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

/// Disabling uploads still permits local ZIPs, with invalid paths and collisions leaving existing files intact.
#[tokio::test]
async fn feedback_save_is_local_and_never_overwrites() -> Result<()> {
    let proxy = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_uri = format!("http://{}", proxy.local_addr()?);
    let mut app_server = TestAppServer::builder()
        .with_args(&["-c", "feedback.enabled=false"])
        .with_env_overrides(&[
            ("HTTPS_PROXY", Some(proxy_uri.as_str())),
            ("https_proxy", Some(proxy_uri.as_str())),
            ("NO_PROXY", Some("")),
            ("no_proxy", Some("")),
        ])
        .build_initialized()
        .await?;
    let request = app_server
        .send_raw_request("feedback/upload", Some(json!({ "classification": "bug" })))
        .await?;
    let error = app_server
        .read_stream_until_error_message(RequestId::Integer(request))
        .await?;
    assert_eq!(
        error.error.message,
        "sending feedback is disabled by configuration"
    );

    let directory = tempfile::tempdir()?;
    let saved = directory.path().join("local.zip");
    std::fs::write(&saved, b"existing export")?;
    let foreign = if cfg!(windows) {
        "/tmp/foreign.zip"
    } else {
        r"C:\Users\test\foreign.zip"
    };
    for (path, message) in [
        (json!("relative.zip"), "absolute host path"),
        (json!("~/feedback.zip"), "absolute host path"),
        (json!(foreign), "absolute host path"),
        (json!(&saved), "already exists"),
        (
            json!(directory.path().join("missing/feedback.zip")),
            "failed to save feedback",
        ),
    ] {
        let request = app_server
            .send_raw_request(
                "feedback/save",
                Some(json!({
                    "classification": "bug", "savePath": path,
                })),
            )
            .await?;
        let error = app_server
            .read_stream_until_error_message(RequestId::Integer(request))
            .await?;
        assert!(error.error.message.contains(message), "{error:?}");
    }
    assert_eq!(std::fs::read(&saved)?, b"existing export");
    assert_eq!(std::fs::read_dir(directory.path())?.count(), 1);
    std::fs::remove_file(&saved)?;
    let request = app_server
        .send_raw_request(
            "feedback/save",
            Some(json!({
                "classification": "bug", "savePath": saved,
            })),
        )
        .await?;
    let response = app_server
        .read_stream_until_response_message(RequestId::Integer(request))
        .await?;
    assert_eq!(response.result["savedPath"], json!(saved));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&saved)?)?;
    let report: serde_json::Value = serde_json::from_reader(archive.by_name("report.json")?)?;
    assert_eq!(report["tags"]["classification"], "bug");
    assert!(
        timeout(Duration::from_millis(100), proxy.accept())
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn feedback_save_preserves_selected_rollout_or_fails_without_upload() -> Result<()> {
    use std::io::Read;

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
    let directory = tempfile::tempdir()?;
    let filename = format!(
        "rollout-2026-10-19T12-00-00-{}.jsonl",
        codex_protocol::ThreadId::new()
    );
    let rollout = directory.path().join(&filename);
    let bytes = b"{\"synthetic\":true}\n";
    std::fs::write(&rollout, bytes)?;
    let saved = directory.path().join("selected.zip");
    let request = app_server.send_raw_request("feedback/save", Some(json!({
        "classification": "bug", "includeLogs": true, "extraLogFiles": [rollout], "savePath": saved,
    }))).await?;
    let response = app_server
        .read_stream_until_response_message(RequestId::Integer(request))
        .await?;
    assert_eq!(response.result["savedPath"], json!(saved));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&saved)?)?;
    let entry = zip
        .file_names()
        .collect::<zip::result::ZipResult<Vec<_>>>()?
        .into_iter()
        .find(|name| name.ends_with(&format!("/{filename}")))
        .expect("selected rollout")
        .to_string();
    let mut actual = Vec::new();
    zip.by_name(&entry)?.read_to_end(&mut actual)?;
    assert_eq!(actual, bytes);
    drop(zip);
    std::fs::remove_file(&rollout)?;
    let missing = directory.path().join("missing.zip");
    let request = app_server.send_raw_request("feedback/save", Some(json!({
        "classification": "bug", "includeLogs": true, "extraLogFiles": [rollout], "savePath": missing,
    }))).await?;
    let error = app_server
        .read_stream_until_error_message(RequestId::Integer(request))
        .await?;
    assert_eq!(error.error.code, -32603);
    assert!(error.error.message.starts_with("failed to save feedback:"));
    assert!(error.error.message.contains("selected attachment"));
    assert!(!missing.exists());
    assert_eq!(std::fs::read_dir(directory.path())?.count(), 1);
    assert!(
        timeout(Duration::from_millis(100), proxy.accept())
            .await
            .is_err()
    );
    Ok(())
}

/// Automatic untouched rollouts are omitted; explicit missing and deleted persisted files fail.
#[tokio::test]
async fn feedback_save_handles_rollout_materialization() -> Result<()> {
    use codex_app_server_protocol::ThreadStartParams;
    use std::io::Read;
    let server = app_test_support::create_mock_responses_server_repeating_assistant("Done").await;
    let home = tempfile::tempdir()?;
    app_test_support::MockResponsesConfig::new(&server.uri()).write(home.path())?;
    let proxy = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_uri = format!("http://{}", proxy.local_addr()?);
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .with_args(&["-c", "feedback.enabled=false"])
        .with_env_overrides(&[
            ("HTTPS_PROXY", Some(proxy_uri.as_str())),
            ("https_proxy", Some(proxy_uri.as_str())),
            ("NO_PROXY", Some("127.0.0.1,localhost")),
            ("no_proxy", Some("127.0.0.1,localhost")),
        ])
        .build_initialized()
        .await?;
    let request = app
        .send_thread_start_request_with_auto_env(ThreadStartParams::default())
        .await?;
    let response = app
        .read_stream_until_response_message(RequestId::Integer(request))
        .await?;
    let thread = response.result["thread"]["id"].as_str().unwrap();
    let rollout = std::path::PathBuf::from(response.result["thread"]["path"].as_str().unwrap());
    assert!(!rollout.exists());
    let output = tempfile::tempdir()?;
    for explicit in [false, true] {
        let destination = output.path().join(format!("fresh-{explicit}.zip"));
        let request = app.send_raw_request("feedback/save", Some(json!({"classification":"bug", "includeLogs":true, "threadId":thread, "extraLogFiles": if explicit { vec![&rollout] } else { vec![] }, "savePath":destination}))).await?;
        if explicit {
            let error = app
                .read_stream_until_error_message(RequestId::Integer(request))
                .await?;
            assert!(error.error.message.contains("selected attachment"));
            assert!(!destination.exists());
        } else {
            app.read_stream_until_response_message(RequestId::Integer(request))
                .await?;
            let mut zip = zip::ZipArchive::new(std::fs::File::open(destination)?)?;
            let names = zip
                .file_names()
                .collect::<zip::result::ZipResult<Vec<_>>>()?
                .into_iter()
                .map(std::borrow::Cow::into_owned)
                .collect::<Vec<_>>();
            assert!(
                !names
                    .iter()
                    .any(|name| name.ends_with(".jsonl") || name.ends_with("rollouts.tar.gz"))
            );
            let index_name = names
                .iter()
                .find(|name| name.ends_with("feedback-thread-index.json"))
                .expect("thread index");
            let index: serde_json::Value = serde_json::from_reader(zip.by_name(index_name)?)?;
            assert_eq!(
                index["threads"][0]["rollout_filename"],
                serde_json::Value::Null
            );
            assert_eq!(index["rollout_archive_filename"], serde_json::Value::Null);
        }
    }
    assert!(!rollout.exists(), "saving must not materialize the rollout");
    let request = app
        .send_raw_request(
            "turn/start",
            Some(json!({"threadId":thread,"input":[{"type":"text","text":"hello"}]})),
        )
        .await?;
    app.read_stream_until_response_message(RequestId::Integer(request))
        .await?;
    app.read_stream_until_notification_message("turn/completed")
        .await?;
    assert!(rollout.exists());
    let expected = std::fs::read(&rollout)?;
    let saved = output.path().join("materialized.zip");
    let request = app.send_raw_request("feedback/save", Some(json!({"classification":"bug", "includeLogs":true, "threadId":thread, "savePath":saved}))).await?;
    app.read_stream_until_response_message(RequestId::Integer(request))
        .await?;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(saved)?)?;
    let suffix = format!("/{}", rollout.file_name().unwrap().to_string_lossy());
    let entry = zip
        .file_names()
        .collect::<zip::result::ZipResult<Vec<_>>>()?
        .into_iter()
        .find(|name| name.ends_with(&suffix))
        .unwrap()
        .into_owned();
    let mut actual = Vec::new();
    zip.by_name(&entry)?.read_to_end(&mut actual)?;
    assert_eq!(actual, expected);
    std::fs::remove_file(&rollout)?;
    let failed = output.path().join("deleted.zip");
    let request = app.send_raw_request("feedback/save", Some(json!({"classification":"bug", "includeLogs":true, "threadId":thread, "savePath":failed}))).await?;
    let error = app
        .read_stream_until_error_message(RequestId::Integer(request))
        .await?;
    assert!(error.error.message.contains("selected attachment"));
    assert!(!failed.exists());
    assert!(
        timeout(Duration::from_millis(100), proxy.accept())
            .await
            .is_err()
    );
    Ok(())
}
