//! Usage permission and earned-reset payloads, including compatibility with older backends.

use super::*;
use crate::types::ConsumeRateLimitResetCreditCode;
use crate::types::RateLimitResetCreditDetails;
use crate::types::RateLimitResetCreditsDetails;
use crate::types::RateLimitResetCreditsSummary;
use codex_model_provider::ClientAuth;
use pretty_assertions::assert_eq;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

#[tokio::test]
async fn ordinary_usage_permission_comes_from_backend_not_display_percent() {
    for (allowed, used_percent) in [(Some(true), 100), (Some(false), 0), (None, 0)] {
        let server = MockServer::start().await;
        let rate_limit = allowed.map(|allowed| {
            serde_json::json!({
                "allowed": allowed, "limit_reached": !allowed,
                "primary_window": {"used_percent": used_percent, "limit_window_seconds": 300,
                    "reset_after_seconds": 60, "reset_at": 2000000000}
            })
        });
        Mock::given(method("GET"))
            .and(path("/api/codex/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "plan_type": "plus", "rate_limit": rate_limit
            })))
            .expect(1)
            .mount(&server)
            .await;
        let response = test_client(&server.uri(), PathStyle::CodexApi)
            .get_rate_limits_with_reset_credits()
            .await
            .unwrap();
        assert_eq!(response.ordinary_usage_allowed, allowed);
    }
}

fn test_client(base_url: &str, path_style: PathStyle) -> Client {
    Client {
        base_url: base_url.to_string(),
        http: codex_http_client::RouteAwareClientPool::new(
            codex_http_client::HttpClientFactory::new(
                codex_http_client::OutboundProxyPolicy::ReqwestDefault,
            ),
            codex_http_client::ClientRouteClass::Api,
        ),
        auth: ClientAuth::new(codex_model_provider::unauthenticated_auth_provider()),
        user_agent: None,
        chatgpt_account_id: None,
        chatgpt_account_is_fedramp: false,
        path_style,
        config_server_base_url: None,
    }
}
