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

// Completion metadata follows its source message without entering later model requests.
#[test_case::test_case(ModeKind::Default; "normal")]
#[test_case::test_case(ModeKind::Plan; "plan")]
#[tokio::test]
async fn native_counts_follow_completed_messages_without_request_replay(
    mode: ModeKind,
) -> Result<()> {
    #[cfg(windows)]
    const READ_TIMEOUT: Duration = Duration::from_secs(25);
    #[cfg(not(windows))]
    const READ_TIMEOUT: Duration = Duration::from_secs(10);

    let server = responses::start_mock_server().await;
    let mut events = vec![responses::ev_response_created("response-1")];
    for (id, text, phase, count) in [
        ("partial", "Partial.", "partial_answer", 0),
        ("final", "Done.", "final_answer", 23),
    ] {
        let mut added = responses::ev_message_item_added(id, "");
        added["item"]["phase"] = json!(phase);
        let mut done = responses::ev_assistant_message(id, text);
        done["item"]["phase"] = json!(phase);
        done["item"]["internal_chat_message_metadata_passthrough"] =
            json!({"content_token_count": count});
        events.extend([added, responses::ev_output_text_delta(text), done]);
    }
    events.push(responses::ev_completed("response-1"));
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut app = expect_startup(
        TestAppServer::builder()
            .with_codex_home(codex_home.path())
            .build_initialized_with_timeout(STARTUP_TIMEOUT),
    )
    .await;
    let response_mock = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(events),
            responses::sse(vec![
                responses::ev_response_created("response-2"),
                responses::ev_assistant_message("next", "Next."),
                responses::ev_completed("response-2"),
            ]),
        ],
    )
    .await;
    let thread = app
        .start_thread(ThreadStartParams {
            model: Some("mock-model".into()),
            ..Default::default()
        })
        .await?
        .thread;
    timeout(READ_TIMEOUT, async {
        for prompt in ["First turn", "Next turn"] {
            let completed = app
                .start_turn_and_wait_for_completion(TurnStartParams {
                    thread_id: thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: prompt.into(),
                        text_elements: Vec::new(),
                    }],
                    collaboration_mode: Some(CollaborationMode {
                        mode,
                        settings: Settings {
                            model: "mock-model".into(),
                            reasoning_effort: None,
                            developer_instructions: None,
                        },
                    }),
                    ..Default::default()
                })
                .await?;
            assert_eq!(completed.turn.status, TurnStatus::Completed);
        }
        let mut counts = Vec::new();
        while counts.len() < 2 {
            let completed: ItemCompletedNotification =
                app.read_notification("item/completed").await?;
            if let ThreadItemPayload::AgentMessage { id, phase, .. } = completed.item.payload {
                counts.push((id, phase, completed.content_token_count));
            }
        }
        assert_eq!(
            counts,
            vec![
                ("partial".into(), Some(MessagePhase::PartialAnswer), Some(0)),
                ("final".into(), Some(MessagePhase::FinalAnswer), Some(23)),
            ]
        );
        app.shutdown_gracefully().await?;
        anyhow::Ok(())
    })
    .await??;
    let requests = response_mock.requests();
    assert_eq!(requests.len(), 2);
    let assistant_input: Vec<_> = requests[1]
        .inputs_of_type("message")
        .into_iter()
        .filter(|item| item["role"] == "assistant")
        .map(|item| {
            (
                item["content"][0]["text"].clone(),
                item["internal_chat_message_metadata_passthrough"]
                    .get("content_token_count")
                    .cloned(),
            )
        })
        .collect();
    assert_eq!(
        assistant_input,
        vec![(json!("Partial."), None), (json!("Done."), None)]
    );
    Ok(())
}

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
                let metrics = provider.as_ref().and_then(|otel| otel.metrics());
                let metrics_enabled = metrics.is_some();
                // Exercise the nonempty export path that concurrent tests can
                // trigger through process-global metrics.
                let record_result = metrics
                    .map(|metrics| {
                        metrics.counter("codex.test.analytics_preferences", /*inc*/ 1, &[])
                    })
                    .transpose();
                if let Some(provider) = provider {
                    // Keep Tokio running while the gRPC exporter flushes; a
                    // synchronous drop can deadlock this current-thread runtime.
                    provider
                        .shutdown_with_timeout(Duration::from_secs(/*secs*/ 5))
                        .await?;
                }
                record_result?;
                assert_eq!(
                    metrics_enabled, expected_metrics,
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
