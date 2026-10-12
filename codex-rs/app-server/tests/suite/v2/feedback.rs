use std::time::Duration;

use anyhow::Result;
use app_test_support::TestAppServer;
use codex_app_server_protocol::RequestId;
use pretty_assertions::assert_eq;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::time::timeout;

#[tokio::test]
async fn external_feedback_is_rejected_before_connecting_or_collecting_logs() -> Result<()> {
    let proxy = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_uri = format!("http://{}", proxy.local_addr()?);
    let mut app_server = TestAppServer::builder()
        .with_env_overrides(&[
            ("HTTPS_PROXY", Some(proxy_uri.as_str())),
            ("https_proxy", Some(proxy_uri.as_str())),
            ("NO_PROXY", Some("")),
            ("no_proxy", Some("")),
        ])
        .build_initialized()
        .await?;

    for include_logs in [false, true] {
        let request_id = app_server
            .send_raw_request(
                "feedback/upload",
                Some(json!({ "classification": "bug", "includeLogs": include_logs })),
            )
            .await?;
        let error = timeout(
            Duration::from_secs(15),
            app_server.read_stream_until_error_message(RequestId::Integer(request_id)),
        )
        .await??;
        assert_eq!(error.error.code, -32600);
        assert!(
            error
                .error
                .message
                .contains("feedback reporting is unavailable in Lemex")
        );
    }
    assert!(
        timeout(Duration::from_millis(100), proxy.accept())
            .await
            .is_err()
    );
    Ok(())
}
