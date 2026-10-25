//! API-key discovery opt-outs must gate network requests and cached catalog authority.

use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn api_key_ultra_is_available_in_picker_and_inference_without_rewriting_catalog() {
    let model = remote_model("provider/reasoner", "Provider reasoner", 0);
    let catalog = ModelsResponse {
        models: vec![model.clone()],
    };
    let manager = StaticModelsManager::new(
        Some(AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
            "test-key",
        ))),
        catalog.clone(),
    );
    let preset = manager.try_list_models().unwrap().remove(0);
    assert!(
        preset
            .supported_reasoning_efforts
            .iter()
            .any(|option| option.effort == codex_protocol::openai_models::ReasoningEffort::Ultra)
    );
    let resolved = manager
        .get_model_info(&model.slug, &ModelsManagerConfig::default())
        .await;
    assert_eq!(
        resolved.multi_agent_version,
        Some(codex_protocol::protocol::MultiAgentVersion::V2)
    );
    assert_eq!(
        resolved.resolve_reasoning_effort(codex_protocol::openai_models::ReasoningEffort::Ultra),
        codex_protocol::openai_models::ReasoningEffort::Medium,
    );
    assert_eq!(
        manager
            .raw_model_catalog(RefreshStrategy::Offline, DEFAULT_HTTP_CLIENT_FACTORY)
            .await,
        catalog
    );
}

#[tokio::test]
async fn api_key_discovery_disabled_preserves_command_auth_discovery_and_merging() {
    let models = vec![remote_model(
        "command-auth",
        "Command Auth",
        /*priority*/ 0,
    )];
    let endpoint = Arc::new(TestModelsEndpoint {
        has_command_auth: true,
        responses: Mutex::new(vec![Ok(models.clone())].into()),
        ..TestModelsEndpoint::default()
    });
    let manager = OpenAiModelsManager::new_without_cache(
        endpoint.clone(),
        Some(AuthManager::from_auth_for_testing(CodexAuth::from_api_key(
            "test-key",
        ))),
    );
    let mut merged = load_remote_models_from_file().unwrap();
    merged.extend(models.clone());
    assert_eq!(
        manager
            .raw_model_catalog(RefreshStrategy::Online, DEFAULT_HTTP_CLIENT_FACTORY)
            .await
            .models,
        merged
    );
    assert_eq!(endpoint.fetch_count(), 1);
}

#[tokio::test]
async fn api_key_discovery_flag_controls_fetches_and_cached_catalogs() {
    let home = tempdir().unwrap();
    let models = vec![remote_model("dynamic", "Dynamic", /*priority*/ 0)];
    let endpoint = TestModelsEndpoint::without_refresh(vec![models.clone()]);
    let auth = AuthManager::from_auth_for_testing(CodexAuth::from_api_key("test-key"));
    let manager =
        OpenAiModelsManager::new(home.path().into(), endpoint.clone(), Some(auth.clone()));
    let bundled = load_remote_models_from_file().unwrap();
    let config = ModelsManagerConfig::default();

    for strategy in [
        RefreshStrategy::Online,
        RefreshStrategy::OnlineIfUncached,
        RefreshStrategy::Offline,
    ] {
        assert_eq!(
            manager
                .raw_model_catalog(strategy, DEFAULT_HTTP_CLIENT_FACTORY)
                .await
                .models,
            bundled
        );
    }
    assert_eq!(endpoint.fetch_count(), 0);

    let manager =
        OpenAiModelsManager::new(home.path().into(), endpoint.clone(), Some(auth.clone()));
    manager.set_api_key_model_discovery_enabled(/*enabled*/ true);
    assert_eq!(
        manager
            .raw_model_catalog(RefreshStrategy::Online, DEFAULT_HTTP_CLIENT_FACTORY)
            .await
            .models,
        models
    );
    assert_eq!(endpoint.fetch_count(), 1);
    assert_eq!(
        manager.get_model_info("dynamic", &config).await,
        model_info::with_local_ultra(models[0].clone())
    );

    // A runtime rollback must also suppress the already-loaded catalog.
    manager.set_api_key_model_discovery_enabled(/*enabled*/ false);
    assert_eq!(
        manager
            .raw_model_catalog(RefreshStrategy::Online, DEFAULT_HTTP_CLIENT_FACTORY)
            .await
            .models,
        bundled
    );
    assert_eq!(manager.try_get_remote_models().unwrap(), bundled);
    assert!(
        manager
            .get_model_info("dynamic", &config)
            .await
            .used_fallback_model_metadata
    );
    assert_eq!(endpoint.fetch_count(), 1);

    // A new session must honor its startup flag even when a matching disk cache exists.
    for enabled in [false, true] {
        let restarted =
            OpenAiModelsManager::new(home.path().into(), endpoint.clone(), Some(auth.clone()));
        restarted.set_api_key_model_discovery_enabled(enabled);
        let expected = if enabled { &models } else { &bundled };
        for strategy in [RefreshStrategy::Offline, RefreshStrategy::OnlineIfUncached] {
            assert_eq!(
                &restarted
                    .raw_model_catalog(strategy, DEFAULT_HTTP_CLIENT_FACTORY)
                    .await
                    .models,
                expected
            );
        }
        assert_eq!(&restarted.try_get_remote_models().unwrap(), expected);
    }
    assert_eq!(endpoint.fetch_count(), 1);
}
