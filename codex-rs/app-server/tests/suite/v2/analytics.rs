use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::write_chatgpt_auth;
use codex_config::types::AuthCredentialsStoreMode;
use codex_config::types::OtelExporterKind;
use codex_config::types::OtelHttpProtocol;
use codex_core::config::ConfigBuilder;
use pretty_assertions::assert_eq;
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;
use wiremock::MockServer;

const SERVICE_VERSION: &str = "0.0.0-test";

#[tokio::test]
async fn metrics_exporters_require_explicit_configuration() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut config = ConfigBuilder::default()
        .codex_home(codex_home.path().to_path_buf())
        .build()
        .await?;
    for exporter in [
        OtelExporterKind::None,
        OtelExporterKind::Statsig,
        OtelExporterKind::OtlpHttp {
            endpoint: "http://localhost:4318".to_string(),
            headers: HashMap::new(),
            protocol: OtelHttpProtocol::Json,
            tls: None,
        },
        OtelExporterKind::OtlpGrpc {
            endpoint: "http://localhost:4317".to_string(),
            headers: HashMap::new(),
            tls: None,
        },
    ] {
        config.otel.metrics_exporter = exporter;
        for analytics_enabled in [None, Some(false), Some(true)] {
            config.analytics_enabled = analytics_enabled;
            for default_analytics_enabled in [false, true] {
                let expected_metrics = match config.otel.metrics_exporter {
                    OtelExporterKind::None => false,
                    OtelExporterKind::Statsig => false,
                    OtelExporterKind::OtlpHttp { .. } | OtelExporterKind::OtlpGrpc { .. } => true,
                };
                let provider = codex_core::otel_init::build_provider(
                    &config,
                    SERVICE_VERSION,
                    Some("codex-app-server"),
                    default_analytics_enabled,
                )
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
                assert_eq!(
                    provider.as_ref().and_then(|otel| otel.metrics()).is_some(),
                    expected_metrics,
                    "exporter={:?}, analytics={analytics_enabled:?}, default={default_analytics_enabled}",
                    config.otel.metrics_exporter,
                );
            }
        }
    }

    Ok(())
}

pub(crate) async fn mount_authenticated_backend(
    server: &MockServer,
    codex_home: &Path,
) -> Result<()> {
    write_chatgpt_auth(
        codex_home,
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    app_test_support::mount_workspace_routing(server).await;
    Ok(())
}
