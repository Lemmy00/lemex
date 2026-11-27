use super::bedrock_auth::BedrockProviderConfig;
use super::bedrock_auth::clear_user_model_provider_if_bedrock;
use super::bedrock_auth::prepare_bedrock_provider;
use super::config_processor::map_error as map_config_error;
use super::config_processor::refresh_guardian_policy;
use super::*;
use crate::auth_mode::auth_mode_to_api;
use crate::model_catalog::ModelCatalog;
use crate::outgoing_message::AccountNotification;
use chrono::DateTime;
use codex_app_server_protocol::BedrockProviderId;
use codex_app_server_protocol::GetAccountRateLimitsParams;
use codex_core::McpRefreshReason;
use codex_login::login_with_bedrock_access_keys;
use codex_mcp::ema_auth_scope;
use codex_model_provider::is_supported_amazon_bedrock_region;
use codex_rmcp_client::EnterpriseOAuthCredentialGuard;

mod backend_client;
mod bedrock_gov_cloud;
mod bedrock_setup;
mod enterprise_login;
mod gateway_oauth;
mod gcp_setup;
mod rate_limit_resets;
mod security_setup;
mod voice_settings;
mod workspace_routing;
mod workspace_switching;

pub(super) use enterprise_login::EnterpriseLoginCompletion;
pub(super) use enterprise_login::EnterpriseLoginTarget;
pub(crate) use gcp_setup::handle_gcp_setup;

enum RefreshTokenRequestOutcome {
    NotAttemptedOrSucceeded,
    FailedTransiently,
    FailedPermanently,
}

enum CloudLoginProvider {
    AmazonBedrock,
    Gcp,
}

enum BedrockLoginCredentials {
    ApiKey(String),
    AccessKeys {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
    },
}

#[derive(Clone)]
pub(crate) struct AccountRequestProcessor {
    model_catalog: Arc<ModelCatalog>,
    auth_manager: Arc<AuthManager>,
    thread_manager: Arc<ThreadManager>,
    outgoing: Arc<OutgoingMessageSender>,
    config: Arc<Config>,
    config_manager: ConfigManager,
    workspace_routing: Arc<Mutex<Option<workspace_routing::CachedWorkspaceRouting>>>,
    workspace_routing_fetches: Arc<Mutex<workspace_routing::WorkspaceRoutingFetches>>,
    workspace_routing_shutdown: CancellationToken,
    gateway_login: Arc<std::sync::Mutex<Option<gateway_oauth::ActiveGatewayLogin>>>,
    gateway_clients:
        Arc<std::sync::Mutex<HashMap<String, Arc<codex_model_provider::GatewayAuthManager>>>>,
    _gateway_notifications: Arc<tokio_util::task::AbortOnDropHandle<()>>,
    pub(super) enterprise_login: Arc<enterprise_login::EnterpriseLoginState>,
}

impl AccountRequestProcessor {
    pub(crate) fn new(
        auth_manager: Arc<AuthManager>,
        thread_manager: Arc<ThreadManager>,
        outgoing: Arc<OutgoingMessageSender>,
        config: Arc<Config>,
        config_manager: ConfigManager,
        model_catalog: Arc<ModelCatalog>,
    ) -> Arc<Self> {
        let gateway_notifications = crate::gateway_oauth_notifications::spawn(
            Arc::clone(&auth_manager),
            config_manager.clone(),
            Arc::clone(&outgoing),
        );
        let enterprise_login = Arc::new(enterprise_login::EnterpriseLoginState::new(
            Arc::clone(&auth_manager),
            Arc::clone(&thread_manager),
            config_manager.clone(),
        ));
        let processor = Arc::new(Self {
            model_catalog,
            _gateway_notifications: Arc::new(gateway_notifications),
            auth_manager,
            thread_manager,
            outgoing,
            config,
            config_manager,
            gateway_login: Arc::new(std::sync::Mutex::new(/*t*/ None)),
            gateway_clients: Arc::new(std::sync::Mutex::new(HashMap::new())),
            workspace_routing: Arc::new(Mutex::new(None)),
            workspace_routing_fetches: Arc::new(Mutex::new(HashMap::new())),
            workspace_routing_shutdown: CancellationToken::new(),
            enterprise_login,
        });
        let resolver: Arc<dyn codex_login::WorkspaceRoutingResolver> = processor.clone();
        processor
            .auth_manager
            .set_workspace_routing_resolver(Arc::downgrade(&resolver));
        let startup = processor.clone();
        tokio::spawn(async move {
            let _ = startup
                .read_account_for_background_workspace_routing()
                .await;
        });
        processor
    }

    pub(crate) async fn login_account(
        &self,
        request_id: ConnectionRequestId,
        params: LoginAccountParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.login_v2(request_id, params).await.map(|()| None)
    }

    pub(crate) async fn logout_account(
        &self,
        request_id: ConnectionRequestId,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.logout_v2(request_id).await.map(|()| None)
    }

    pub(crate) async fn cancel_login_account(
        &self,
        params: CancelLoginAccountParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.cancel_login_response(params)
            .await
            .map(|response| Some(response.into()))
    }

    pub(crate) async fn get_auth_status(
        &self,
        params: GetAuthStatusParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.get_auth_status_response(params)
            .await
            .map(|response| Some(response.into()))
    }

    pub(crate) async fn get_account_rate_limits(
        &self,
        params: Option<GetAccountRateLimitsParams>,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.get_account_rate_limits_response(params.unwrap_or_default())
            .await
            .map(|response| Some(response.into()))
    }

    pub(crate) async fn get_account_token_usage(
        &self,
        params: Option<GetAccountTokenUsageParams>,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        Err(invalid_request(
            "Account reporting and credit promotions are unavailable in Lemex. Local coding token usage remains available in session history and /status.",
        ))
    }

    pub(crate) async fn get_workspace_messages(
        &self,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        Err(invalid_request(
            "Account reporting and credit promotions are unavailable in Lemex. Local coding token usage remains available in session history and /status.",
        ))
    }

    pub(crate) async fn send_add_credits_nudge_email(
        &self,
        params: SendAddCreditsNudgeEmailParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        Err(invalid_request(
            "Account reporting and credit promotions are unavailable in Lemex. Local coding token usage remains available in session history and /status.",
        ))
    }

    pub(crate) async fn cancel_active_login(&self) {
        self.cancel_gateway_login();
        self.enterprise_login.cancel(/*login_id*/ None).await;
    }

    pub(crate) fn clear_external_auth(&self) {
        self.workspace_routing_shutdown.cancel();
        self.auth_manager.clear_external_auth();
    }

    fn current_account_updated_notification(&self) -> AccountUpdatedNotification {
        let auth = self.auth_manager.auth_cached();
        AccountUpdatedNotification {
            reason: None,
            auth_mode: auth
                .as_ref()
                .map(CodexAuth::api_auth_mode)
                .map(auth_mode_to_api),
            plan_type: auth.as_ref().and_then(CodexAuth::account_plan_type),
        }
    }

    async fn load_latest_config(&self) -> Config {
        match self
            .config_manager
            .load_latest_config(/*fallback_cwd*/ None)
            .await
        {
            Ok(config) => config,
            Err(err) => {
                tracing::warn!("failed to reload config, using startup config: {err}");
                self.config.as_ref().clone()
            }
        }
    }

    async fn maybe_refresh_plugin_caches_for_current_config(
        config_manager: &ConfigManager,
        thread_manager: &Arc<ThreadManager>,
        auth: Option<CodexAuth>,
    ) {
        thread_manager
            .plugins_manager()
            .clear_recommended_plugins_cache();

        match config_manager
            .load_latest_config(/*fallback_cwd*/ None)
            .await
        {
            Ok(config) => {
                let runtime_auth = thread_manager.runtime_auth_for_config(&config);
                Self::spawn_effective_plugins_changed_task(
                    Arc::clone(thread_manager),
                    config_manager.clone(),
                    McpRefreshReason::AccountPluginCacheChanged,
                );
                let plugins_config = config.plugins_config_input();
                let refresh_thread_manager = Arc::clone(thread_manager);
                let refresh_config_manager = config_manager.clone();
                let on_effective_plugins_changed: Arc<
                    dyn Fn(codex_core_plugins::EffectivePluginsChange) + Send + Sync,
                > = Arc::new(move |change| {
                    Self::spawn_effective_plugins_changed_task(
                        Arc::clone(&refresh_thread_manager),
                        refresh_config_manager.clone(),
                        change.metadata_change_id.map_or(
                            McpRefreshReason::AccountPluginCacheChanged,
                            McpRefreshReason::InstalledPluginMetadata,
                        ),
                    );
                });
                thread_manager
                    .plugins_manager()
                    .maybe_start_curated_repo_sync_for_config(
                        &plugins_config,
                        Some(Arc::clone(&on_effective_plugins_changed)),
                    );
                if let Err(err) = runtime_auth {
                    warn!("failed to initialize runtime auth for remote plugin refresh: {err}");
                    return;
                }
                thread_manager
                    .plugins_manager()
                    .maybe_start_remote_plugin_caches_refresh(
                        &plugins_config,
                        auth,
                        Some(on_effective_plugins_changed),
                    );
            }
            Err(err) => {
                warn!(
                    "failed to reload config after account changed, skipping remote installed plugins cache refresh: {err}"
                );
            }
        }
    }

    fn spawn_effective_plugins_changed_task(
        thread_manager: Arc<ThreadManager>,
        config_manager: ConfigManager,
        reason: McpRefreshReason,
    ) {
        tokio::spawn(async move {
            thread_manager.plugins_manager().clear_cache();
            thread_manager.skills_service().clear_cache();
            crate::mcp_refresh::reload_mcp_config_best_effort(&thread_manager, &config_manager)
                .await;
            thread_manager.invalidate_mcp_runtimes_for(reason).await;
        });
    }

    async fn login_v2(
        &self,
        request_id: ConnectionRequestId,
        params: LoginAccountParams,
    ) -> Result<(), JSONRPCErrorError> {
        if matches!(
            &params,
            LoginAccountParams::Chatgpt { .. }
                | LoginAccountParams::ChatgptDeviceCode
                | LoginAccountParams::ChatgptAuthTokens { .. }
        ) {
            return Err(invalid_request(
                "Lemex uses API keys. Run `lemex setup` or set LEMEX_API_KEY.",
            ));
        }
        if self.auth_manager.is_workload_identity_selected() {
            return Err(self.configured_auth_owned_by_host_error());
        }
        self.enterprise_login.cancel(/*login_id*/ None).await;
        match params {
            LoginAccountParams::Chatgpt { .. }
            | LoginAccountParams::ChatgptDeviceCode
            | LoginAccountParams::ChatgptAuthTokens { .. } => {
                return Err(invalid_request(
                    "Lemex uses API keys. Run `lemex setup` or set LEMEX_API_KEY.",
                ));
            }
            LoginAccountParams::ApiKey { api_key } => {
                self.login_api_key_v2(request_id, LoginApiKeyParams { api_key })
                    .await;
            }
            LoginAccountParams::AmazonBedrock {
                api_key,
                region,
                provider_id,
            } => {
                self.login_amazon_bedrock_v2(
                    request_id,
                    BedrockLoginCredentials::ApiKey(api_key),
                    region,
                    provider_id.unwrap_or(BedrockProviderId::AmazonBedrock),
                )
                .await;
            }
            LoginAccountParams::AmazonBedrockAccessKeys {
                access_key_id,
                secret_access_key,
                session_token,
                region,
                provider_id,
            } => {
                self.login_amazon_bedrock_v2(
                    request_id,
                    BedrockLoginCredentials::AccessKeys {
                        access_key_id,
                        secret_access_key,
                        session_token,
                    },
                    region,
                    provider_id.unwrap_or(BedrockProviderId::AmazonBedrock),
                )
                .await;
            }
        }
        Ok(())
    }

    fn external_auth_active_error(&self) -> JSONRPCErrorError {
        invalid_request(
            "External auth is active. Use account/login/start (chatgptAuthTokens) to update it or account/logout to clear it.",
        )
    }

    fn configured_auth_owned_by_host_error(&self) -> JSONRPCErrorError {
        invalid_request(
            "Configured external authentication is owned by the app-server host and cannot be changed through account RPCs.",
        )
    }

    fn ensure_provider_login_allowed(
        &self,
        provider: CloudLoginProvider,
    ) -> Result<(), JSONRPCErrorError> {
        if self.auth_manager.is_workload_identity_selected() {
            return Err(self.configured_auth_owned_by_host_error());
        }
        if self.auth_manager.is_external_chatgpt_auth_active() {
            return Err(self.external_auth_active_error());
        }
        if !self
            .auth_manager
            .is_login_method_allowed(ForcedLoginMethod::Api)
        {
            let operation = match provider {
                CloudLoginProvider::AmazonBedrock => "Amazon Bedrock login",
                CloudLoginProvider::Gcp => "Google Cloud setup",
            };
            return Err(invalid_request(format!(
                "{operation} is disabled. Use ChatGPT login instead."
            )));
        }
        Ok(())
    }

    async fn login_api_key_common(
        &self,
        params: &LoginApiKeyParams,
    ) -> std::result::Result<(), JSONRPCErrorError> {
        if self.auth_manager.is_external_chatgpt_auth_active() {
            return Err(self.external_auth_active_error());
        }

        if !self
            .auth_manager
            .is_login_method_allowed(ForcedLoginMethod::Api)
        {
            return Err(invalid_request(
                "API key login is disabled. Use ChatGPT login instead.",
            ));
        }

        match login_with_api_key(
            &self.config.codex_home,
            &params.api_key,
            self.config.cli_auth_credentials_store_mode,
            self.config.auth_keyring_backend_kind(),
        ) {
            Ok(()) => {
                self.auth_manager.reload().await;
                self.config_manager.clear_chatgpt_config_loader();
                Ok(())
            }
            Err(err) => Err(internal_error(format!("failed to save api key: {err}"))),
        }
    }

    async fn login_api_key_v2(&self, request_id: ConnectionRequestId, params: LoginApiKeyParams) {
        let result = self
            .login_api_key_common(&params)
            .await
            .map(|()| LoginAccountResponse::ApiKey {});
        let logged_in = result.is_ok();
        if logged_in {
            self.model_catalog.invalidate().await;
        }
        self.outgoing.send_result(request_id, result).await;

        if logged_in {
            self.send_login_success_notifications(/*login_id*/ None)
                .await;
        }
    }

    async fn login_amazon_bedrock_v2(
        &self,
        request_id: ConnectionRequestId,
        credentials: BedrockLoginCredentials,
        region: String,
        provider_id: BedrockProviderId,
    ) {
        let result = async {
            self.ensure_provider_login_allowed(CloudLoginProvider::AmazonBedrock)?;

            match &credentials {
                BedrockLoginCredentials::ApiKey(api_key) => {
                    if api_key.trim().is_empty() {
                        return Err(invalid_request("Amazon Bedrock API key must not be empty."));
                    }
                }
                BedrockLoginCredentials::AccessKeys {
                    access_key_id,
                    secret_access_key,
                    ..
                } => {
                    if access_key_id.trim().is_empty() || secret_access_key.trim().is_empty() {
                        return Err(invalid_request(
                            "AWS access key ID and secret access key must not be empty.",
                        ));
                    }
                }
            }
            let region = region.trim();
            if !is_supported_amazon_bedrock_region(region) {
                return Err(invalid_request(format!(
                    "Amazon Bedrock does not support region `{region}`"
                )));
            }

            self.cancel_active_login().await;
            let write = prepare_bedrock_provider(
                &self.config_manager,
                BedrockProviderConfig {
                    provider_id,
                    region: matches!(&credentials, BedrockLoginCredentials::AccessKeys { .. })
                        .then_some(region),
                    profile: None,
                },
            )
            .await?;
            self.config_manager
                .commit_provider_config(write)
                .await
                .map_err(map_config_error)?;

            match credentials {
                BedrockLoginCredentials::ApiKey(api_key) => login_with_bedrock_api_key(
                    &self.config.codex_home,
                    api_key.trim(),
                    region,
                    self.config.cli_auth_credentials_store_mode,
                    self.config.auth_keyring_backend_kind(),
                ),
                BedrockLoginCredentials::AccessKeys {
                    access_key_id,
                    secret_access_key,
                    session_token,
                } => {
                    let session_token = session_token
                        .as_deref()
                        .map(str::trim)
                        .filter(|token| !token.is_empty());
                    login_with_bedrock_access_keys(
                        &self.config.codex_home,
                        access_key_id.trim(),
                        secret_access_key.trim(),
                        session_token,
                        self.config.cli_auth_credentials_store_mode,
                        self.config.auth_keyring_backend_kind(),
                    )
                }
            }
            .map_err(|err| internal_error(format!("failed to save Amazon Bedrock auth: {err}")))?;
            self.auth_manager.reload().await;
            self.config_manager.clear_chatgpt_config_loader();
            Ok(LoginAccountResponse::AmazonBedrock {})
        }
        .await;
        let logged_in = result.is_ok();
        if logged_in {
            self.model_catalog.invalidate().await;
        }
        self.outgoing.send_result(request_id, result).await;

        if logged_in {
            self.send_login_success_notifications(/*login_id*/ None)
                .await;
        }
    }

    async fn cancel_login_response(
        &self,
        params: CancelLoginAccountParams,
    ) -> Result<CancelLoginAccountResponse, JSONRPCErrorError> {
        let login_id = params.login_id;
        let _ = Uuid::parse_str(&login_id)
            .map_err(|_| invalid_request(format!("invalid login id: {login_id}")))?;
        if self.enterprise_login.cancel(Some(&login_id)).await {
            return Ok(CancelLoginAccountResponse {
                status: CancelLoginAccountStatus::Canceled,
            });
        }
        Ok(CancelLoginAccountResponse {
            status: CancelLoginAccountStatus::NotFound,
        })
    }

    async fn send_login_success_notifications(&self, login_id: Option<Uuid>) {
        self.thread_manager
            .invalidate_mcp_runtimes_for(McpRefreshReason::AccountLoginCompleted)
            .await;
        self.send_account_login_notifications(AccountLoginCompletedNotification {
            login_id: login_id.map(|id| id.to_string()),
            success: true,
            error: None,
            onboarding_entrypoint: None,
        })
        .await;
    }

    async fn send_account_login_notifications(
        &self,
        mut payload: AccountLoginCompletedNotification,
    ) {
        let auth_changes = self.auth_manager.auth_change_state_receiver();
        let owner_generation = auth_changes.borrow().owner_generation;
        let logged_in = payload.success;
        if logged_in && let Err(error) = self.read_account(/*request*/ None).await {
            payload.success = false;
            payload.error = Some(error.to_string());
        }
        if logged_in && auth_changes.borrow().owner_generation == owner_generation {
            refresh_guardian_policy(&self.thread_manager).await;
        }
        if logged_in && auth_changes.borrow().owner_generation == owner_generation {
            Self::maybe_refresh_plugin_caches_for_current_config(
                &self.config_manager,
                &self.thread_manager,
                self.auth_manager.auth_cached(),
            )
            .await;
        }

        self.outgoing
            .send_account_notification(
                /*connection_id*/ None,
                &auth_changes,
                owner_generation,
                AccountNotification::LoginCompleted(payload),
            )
            .await;

        if logged_in {
            let notification = self.current_account_updated_notification();
            self.outgoing
                .send_account_notification(
                    /*connection_id*/ None,
                    &auth_changes,
                    owner_generation,
                    AccountNotification::Updated(notification),
                )
                .await;
        }
    }

    async fn logout_common(&self) -> std::result::Result<Option<AuthMode>, JSONRPCErrorError> {
        if self.auth_manager.is_workload_identity_selected() {
            return Err(self.configured_auth_owned_by_host_error());
        }
        self.cancel_active_login().await;
        let config =
            logout_configured_account(&self.config_manager, &self.auth_manager, &self.config)
                .await?;
        self.thread_manager
            .invalidate_mcp_runtimes_for(McpRefreshReason::AccountLoggedOut)
            .await;
        clear_account_configuration(&self.config_manager, &config).await?;

        self.model_catalog.invalidate().await;
        *self.workspace_routing.lock().await = None;

        refresh_guardian_policy(&self.thread_manager).await;
        Self::maybe_refresh_plugin_caches_for_current_config(
            &self.config_manager,
            &self.thread_manager,
            self.auth_manager.auth_cached(),
        )
        .await;

        // Reflect the current auth method after logout (likely None).
        Ok(self
            .auth_manager
            .auth_cached()
            .as_ref()
            .map(CodexAuth::api_auth_mode)
            .map(auth_mode_to_api))
    }

    async fn logout_v2(&self, request_id: ConnectionRequestId) -> Result<(), JSONRPCErrorError> {
        let result = self.logout_common().await;
        let account_updated =
            result
                .as_ref()
                .ok()
                .cloned()
                .map(|auth_mode| AccountUpdatedNotification {
                    reason: None,
                    auth_mode,
                    plan_type: None,
                });
        self.outgoing
            .send_result(request_id, result.map(|_| LogoutAccountResponse {}))
            .await;

        if let Some(payload) = account_updated {
            self.outgoing
                .send_server_notification(ServerNotification::AccountUpdated(payload))
                .await;
        }
        Ok(())
    }

    async fn refresh_token_if_requested(&self, do_refresh: bool) -> RefreshTokenRequestOutcome {
        if self.auth_manager.is_external_chatgpt_auth_active() {
            return RefreshTokenRequestOutcome::NotAttemptedOrSucceeded;
        }
        if do_refresh && let Err(err) = self.auth_manager.refresh_token().await {
            let failed_reason = err.failed_reason();
            if failed_reason.is_none() {
                tracing::warn!("failed to refresh token while getting account: {err}");
                return RefreshTokenRequestOutcome::FailedTransiently;
            }
            return RefreshTokenRequestOutcome::FailedPermanently;
        }
        RefreshTokenRequestOutcome::NotAttemptedOrSucceeded
    }

    async fn get_auth_status_response(
        &self,
        params: GetAuthStatusParams,
    ) -> Result<GetAuthStatusResponse, JSONRPCErrorError> {
        let include_token = params.include_token.unwrap_or(false);
        let do_refresh = params.refresh_token.unwrap_or(false);

        self.refresh_token_if_requested(do_refresh).await;

        // Determine whether auth is required based on the active model provider.
        // If a custom provider is configured with `requires_openai_auth == false`,
        // then no auth step is required; otherwise, default to requiring auth.
        let config = self.load_latest_config().await;
        let requires_openai_auth = config.model_provider.requires_openai_auth;

        let response = if !requires_openai_auth {
            GetAuthStatusResponse {
                auth_method: None,
                auth_token: None,
                requires_openai_auth: Some(false),
            }
        } else {
            let auth = if do_refresh {
                self.auth_manager.auth_cached()
            } else {
                self.auth_manager.auth().await
            };
            match auth {
                Some(auth) => {
                    let permanent_refresh_failure =
                        self.auth_manager.refresh_failure_for_auth(&auth).is_some();
                    let auth_mode = auth_mode_to_api(auth.api_auth_mode());
                    let (reported_auth_method, token_opt) =
                        if self.auth_manager.is_workload_identity_selected()
                            || matches!(
                                auth,
                                CodexAuth::Headers(_)
                                    | CodexAuth::AgentIdentity(_)
                                    | CodexAuth::PersonalAccessToken(_)
                            )
                            || include_token && permanent_refresh_failure
                        {
                            // Host-owned and metadata-bearing credentials are never exported.
                            (Some(auth_mode), None)
                        } else {
                            match auth.get_token() {
                                Ok(token) if !token.is_empty() => {
                                    let tok = if include_token { Some(token) } else { None };
                                    (Some(auth_mode), tok)
                                }
                                Ok(_) => (None, None),
                                Err(err) => {
                                    tracing::warn!("failed to get token for auth status: {err}");
                                    (None, None)
                                }
                            }
                        };
                    GetAuthStatusResponse {
                        auth_method: reported_auth_method,
                        auth_token: token_opt,
                        requires_openai_auth: Some(true),
                    }
                }
                None => GetAuthStatusResponse {
                    auth_method: None,
                    auth_token: None,
                    requires_openai_auth: Some(true),
                },
            }
        };

        Ok(response)
    }

    async fn get_account_rate_limits_response(
        &self,
        params: GetAccountRateLimitsParams,
    ) -> Result<GetAccountRateLimitsResponse, JSONRPCErrorError> {
        let Some((auth, http_client_factory)) =
            self.auth_manager.auth_with_http_client_factory().await
        else {
            return Err(invalid_request(
                "codex account authentication required to read rate limits",
            ));
        };

        if !auth.uses_codex_backend() {
            return Err(invalid_request(
                "chatgpt authentication required to read rate limits",
            ));
        }

        let client = self.backend_client(&auth, http_client_factory).await?;

        let usage_request = async {
            if params.supports_luna_reserve
                && auth.auth_mode() == codex_protocol::auth::AuthMode::Chatgpt
                && !auth.is_fedramp_account()
            {
                client.get_rate_limits_with_luna_reserve().await
            } else {
                client.get_rate_limits_with_reset_credits().await
            }
        };
        let (response, detailed_rate_limit_reset_credits) = tokio::join!(usage_request, async {
            if params.exclude_reset_credit_details {
                None
            } else {
                Self::detailed_rate_limit_reset_credits(&client).await
            }
        },);
        let response = response
            .map_err(|err| internal_error(format!("failed to fetch codex rate limits: {err}")))?;
        if response.rate_limits.is_empty() {
            return Err(internal_error(
                "failed to fetch codex rate limits: no snapshots returned",
            ));
        }

        let rate_limits_by_limit_id: HashMap<_, _> = response
            .rate_limits
            .iter()
            .cloned()
            .map(|snapshot| {
                let limit_id = snapshot
                    .limit_id
                    .clone()
                    .unwrap_or_else(|| "codex".to_string());
                (limit_id, snapshot)
            })
            .collect();
        let rate_limits = response
            .rate_limits
            .iter()
            .find(|snapshot| snapshot.limit_id.as_deref() == Some("codex"))
            .cloned()
            .unwrap_or_else(|| response.rate_limits[0].clone());

        let rate_limit_reset_credits = detailed_rate_limit_reset_credits.or_else(|| {
            response
                .rate_limit_reset_credits
                .map(|summary| RateLimitResetCreditsSummary {
                    available_count: summary.available_count,
                    credits: None,
                })
        });

        // Match desktop's account readiness check before exposing account-bound CTA content.
        // Normal rate limits remain available when older backends omit identity or banner data.
        // Login can change while the backend read is in flight.
        let active_auth = self.auth_manager.auth().await;
        let matches_active_account = active_auth.is_some_and(|auth| {
            !auth.is_fedramp_account()
                && response.account_id.is_some()
                && response.account_id == auth.get_account_id()
                && response.user_id.is_some()
                && response.user_id == auth.get_chatgpt_user_id()
        });
        let rate_limit_upsell = response
            .rate_limit_upsell
            .filter(|_| matches_active_account);

        Ok(GetAccountRateLimitsResponse {
            ordinary_usage_allowed: response
                .ordinary_usage_allowed
                .filter(|_| matches_active_account),
            rate_limits: rate_limits.into(),
            rate_limits_by_limit_id: Some(
                rate_limits_by_limit_id
                    .into_iter()
                    .map(|(limit_id, snapshot)| (limit_id, snapshot.into()))
                    .collect(),
            ),
            rate_limit_reset_credits,
            account_id: response.account_id,
            rate_limit_upsell,
        })
    }
}

/// Remove account credentials using the current account's policy, even during startup recovery.
pub(crate) async fn logout_configured_account(
    config_manager: &ConfigManager,
    auth_manager: &Arc<AuthManager>,
    fallback_config: &Config,
) -> Result<Config, JSONRPCErrorError> {
    // Another process may have changed the persisted workspace. Reload both
    // account authority and its policy before selecting a grant to remove.
    auth_manager
        .reload_with_cached_auth()
        .await
        .map_err(|err| internal_error(format!("logout failed: {err}")))?;
    config_manager
        .update_managed_config_loader(Arc::clone(auth_manager), fallback_config)
        .await;
    let config = config_manager
        .load_latest_config(/*fallback_cwd*/ None)
        .await;
    let scope = ema_auth_scope(auth_manager.auth_cached().as_ref());
    let enterprise_policy_failed = scope.is_some() && config.is_err();
    // Startup policy may belong to another workspace. Never use that fallback
    // to select a credential; a policy failure must not block primary logout.
    let enterprise_grant = config.as_ref().ok().and_then(|config| {
        scope
            .as_ref()
            .zip(config.mcp_enterprise_managed_auth.as_ref())
            .map(|(scope, profile)| {
                (
                    profile.idp.credential_name(scope),
                    profile.idp.issuer.clone(),
                    config.auth_keyring_backend_kind(),
                )
            })
    });
    let config = config.unwrap_or_else(|_| fallback_config.clone());

    // Retain the credential lock through primary logout. Otherwise a second
    // process can commit after deletion but before the account is removed.
    let enterprise_guard =
        if let Some((credential_name, issuer, keyring_backend)) = enterprise_grant {
            EnterpriseOAuthCredentialGuard::acquire(&credential_name, &issuer, keyring_backend)
                .await
                .map(Some)
        } else {
            Ok(None)
        };
    let cleanup_failed = match &enterprise_guard {
        Ok(Some(guard)) => guard.delete_tokens().is_err(),
        Ok(None) => false,
        Err(_) => true,
    };
    if enterprise_policy_failed || cleanup_failed {
        tracing::warn!("Failed to remove enterprise authorization; continuing account logout");
    }

    match auth_manager.logout_with_revoke().await {
        Ok(_) => {}
        Err(err) => {
            return Err(internal_error(format!("logout failed: {err}")));
        }
    }
    drop(enterprise_guard);
    Ok(config)
}

/// Discard account-scoped cloud and provider settings after credentials have been removed.
pub(crate) async fn clear_account_configuration(
    config_manager: &ConfigManager,
    config: &Config,
) -> Result<(), JSONRPCErrorError> {
    config_manager.clear_chatgpt_config_loader();
    if config.model_provider.is_amazon_bedrock() {
        clear_user_model_provider_if_bedrock(config_manager, config).await?;
    }
    Ok(())
}
