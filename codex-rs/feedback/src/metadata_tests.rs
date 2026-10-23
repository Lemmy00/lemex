//! Exercise local dynamic tag collection and bounded updates.

use crate::CodexFeedback;
use crate::*;
use pretty_assertions::assert_eq;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[test]
fn refreshing_logs_preserves_captured_metadata() {
    let feedback = CodexFeedback::new();
    let _guard = tracing_subscriber::registry()
        .with(feedback.metadata_layer())
        .set_default();
    let thread_id = ThreadId::new();
    tracing::info!(target: FEEDBACK_TAGS_TARGET, approval_policy = "on-request");
    feedback.report_failure("initial log\n");
    let mut snapshot = feedback.snapshot(Some(thread_id));

    // Another conversation changes the process-wide tags while log collection runs.
    tracing::info!(target: FEEDBACK_TAGS_TARGET, approval_policy = "never");
    feedback.report_failure("flush failed\n");
    snapshot.refresh_logs(&feedback);

    assert_eq!(
        (snapshot.tags, snapshot.thread_id, snapshot.bytes),
        (
            BTreeMap::from([("approval_policy".to_string(), "on-request".to_string())]),
            thread_id.to_string(),
            b"initial log\nflush failed\n".to_vec(),
        )
    );
}

#[test]
fn dynamic_tags_remain_available_in_local_snapshots() {
    let feedback = CodexFeedback::new();
    let _guard = tracing_subscriber::registry()
        .with(feedback.metadata_layer())
        .set_default();
    let mut expected: BTreeMap<_, _> = (0..200)
        .map(|index| {
            (
                format!("feature.test_{index}"),
                (index % 2 == 0).to_string(),
            )
        })
        .collect();
    expected.insert("model_context_window".to_string(), "872000".to_string());
    tracing::info!(
        target: FEEDBACK_TAGS_TARGET,
        model = "test-model",
        tags_json = ?tracing::field::display(serde_json::json!(expected)),
    );
    expected.insert("model".to_string(), "test-model".to_string());
    let snapshot = feedback.snapshot(/*session_id*/ None);
    assert_eq!(snapshot.tags, expected);
}

#[test]
fn bounded_dynamic_tags_still_update_existing_values() {
    let feedback = CodexFeedback::new();
    let _guard = tracing_subscriber::registry()
        .with(feedback.metadata_layer())
        .set_default();
    let mut expected: BTreeMap<_, _> = (0..MAX_FEEDBACK_TAGS)
        .map(|index| (format!("feature.test_{index}"), "false".to_string()))
        .collect();
    tracing::info!(target: FEEDBACK_TAGS_TARGET, tags_json = %serde_json::json!(expected));
    tracing::info!(
        target: FEEDBACK_TAGS_TARGET,
        tags_json = %serde_json::json!({
            "feature.overflow": "true",
            "feature.test_0": "true",
        }),
    );
    expected.insert("feature.test_0".to_string(), "true".to_string());
    assert_eq!(feedback.snapshot(/*session_id*/ None).tags, expected);
    tracing::info!(
        target: FEEDBACK_TAGS_TARGET,
        tags_json = %serde_json::json!({"feature.test_0": "false"}),
    );
    expected.insert("feature.test_0".to_string(), "false".to_string());
    assert_eq!(feedback.snapshot(/*session_id*/ None).tags, expected);
}

#[test]
fn local_feedback_export_obeys_log_consent_and_preserves_existing_files() {
    for include_logs in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("feedback.zip");
        let feedback = CodexFeedback::new();
        feedback.report_failure("selected local log");
        let snapshot = feedback.snapshot(None);
        let options = || FeedbackUploadOptions {
            classification: "bug",
            reason: Some("local diagnostics"),
            tags: None,
            include_logs,
            extra_attachments: Vec::new(),
            extra_attachment_paths: &[],
            session_source: None,
            logs_override: None,
        };
        snapshot.save_feedback(options(), &path).unwrap();
        let original = fs::read(&path).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&original)).unwrap();
        let mut report = String::new();
        archive
            .by_name("report.json")
            .unwrap()
            .read_to_string(&mut report)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&report).unwrap()["classification"],
            "bug"
        );
        assert_eq!(
            archive.by_name("attachments/0000/codex-logs.log").is_ok(),
            include_logs
        );
        assert!(snapshot.save_feedback(options(), &path).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
    }
}
