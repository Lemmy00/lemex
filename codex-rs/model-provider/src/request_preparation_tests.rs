use super::adapt_responses_input;
use super::prepare_provider_request;
use crate::create_model_provider;
use crate::provider_uses_first_party_auth_path;
use base64::Engine;
use codex_login::AuthManager;
use codex_login::CodexAuth;
use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn only_rcp_kimi_uses_system_messages_for_instructions() {
    let prompt = json!([
        {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Instructions"}]},
        {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hello"}]},
        {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Update"}]}
    ]);
    let kimi = "moonshotai/Kimi-K3.1-Pro";
    let rcp = ModelProviderInfo::create_rcp_provider(/*base_url*/ None);
    let other = ModelProviderInfo::default();
    for (provider, model, expected_role) in [
        (&rcp, kimi, "system"),
        (&rcp, "moonshotai/Kimi-K3.1-Pro-mxfp4", "system"),
        (&other, kimi, "developer"),
        (&rcp, "Qwen/Qwen3.8-27B", "developer"),
    ] {
        let mut input: Vec<ResponseItem> = serde_json::from_value(prompt.clone()).unwrap();
        adapt_responses_input(provider, model, &mut input);
        let mut expected = prompt.clone();
        expected[0]["role"] = json!(expected_role);
        expected[2]["role"] = json!(expected_role);
        assert_eq!(serde_json::to_value(input).unwrap(), expected, "{model}");
    }
}

#[tokio::test]
async fn explicit_provider_token_survives_a_workspace_switch_during_setup() {
    let home = TempDir::new().unwrap();
    let claims = json!({"https://api.openai.com/auth": {"chatgpt_user_id": "user-a"}});
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
    let token = format!("header.{payload}.signature");
    let initial = CodexAuth::from_external_chatgpt_tokens(
        &token,
        "workspace-a",
        /*chatgpt_plan_type*/ None,
    )
    .unwrap();
    let manager = AuthManager::from_auth_for_testing_with_home(initial, home.path().to_path_buf());
    let mut info = ModelProviderInfo::create_openai_provider(/*base_url*/ None);
    info.experimental_bearer_token = Some("provider-token".into());
    let provider = create_model_provider(info, Some(manager.clone()));

    let mut attempts = 0;
    let resolved = prepare_provider_request(
        provider.as_ref(),
        provider_uses_first_party_auth_path(provider.info()),
        |_| {
            attempts += 1;
            async {
                let (_, resolved) = provider.request_provider_and_auth(/*scope*/ None).await?;
                codex_login::auth::login_with_chatgpt_auth_tokens(
                    home.path(),
                    &token,
                    "workspace-b",
                    /*chatgpt_plan_type*/ None,
                )?;
                manager.reload().await;
                Ok(resolved)
            }
        },
    )
    .await
    .unwrap();

    assert_eq!(
        manager.auth_cached().unwrap().get_account_id().as_deref(),
        Some("workspace-b")
    );
    assert_eq!(attempts, 1);
    let mut headers = http::HeaderMap::new();
    resolved.auth.add_auth_headers(&mut headers).unwrap();
    assert_eq!(
        resolved.credential.as_ref().and_then(CodexAuth::api_key),
        Some("provider-token")
    );
    assert_eq!(
        headers[http::header::AUTHORIZATION],
        "Bearer provider-token"
    );
}
