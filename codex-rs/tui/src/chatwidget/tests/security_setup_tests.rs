use super::*;
use crate::security_setup::Identity;
use crate::security_setup::Notice;
use serde_json::json;

fn setup_notice() -> Notice {
    serde_json::from_value(json!({
        "title":"Keep using Daybreak mode",
        "description":"Set up Advanced Account Security with a hardware security key by October 1. Already Persona-verified? Add your key before October 15 to skip re-verification.",
        "action":{"label":"Set up security","url":"https://chatgpt.com/cyber"}
    }))
    .unwrap()
}

fn setup_identity() -> Identity {
    Identity {
        account: "account".into(),
        user: "user".into(),
    }
}

#[tokio::test]
async fn security_setup_preserves_draft_action_and_dismissal_across_widgets() {
    let (mut chat, _rx, _ops) = make_chatwidget_manual(Some("test-model-a")).await;
    chat.show_security_setup(setup_identity(), setup_notice());
    let rendered = normalize_snapshot_paths(render_bottom_popup(&chat, /*width*/ 70));
    insta::assert_snapshot!("security_setup_banner", rendered);
    let (mut replacement, mut events, _ops) = make_chatwidget_manual(Some("test-model-a")).await;
    chat.app_event_tx
        .voice_only
        .store(/*val*/ true, std::sync::atomic::Ordering::Relaxed);
    replacement.inherit_security_setup(&mut chat);
    chat = replacement;
    assert!(render_bottom_popup(&chat, /*width*/ 70).contains("Set up security"));
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    assert!(
        matches!(events.try_recv(), Ok(AppEvent::OpenUrlInBrowser { url }) if url == "https://chatgpt.com/cyber")
    );
    assert!(render_bottom_popup(&chat, /*width*/ 70).contains("Set up security"));
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!render_bottom_popup(&chat, /*width*/ 70).contains("Set up security"));

    let (mut replacement, _events, _ops) = make_chatwidget_manual(Some("test-model-a")).await;
    replacement.inherit_security_setup(&mut chat);
    replacement.show_security_setup(setup_identity(), setup_notice());
    assert_eq!(
        replacement.bottom_pane.inline_banner_lifecycle(),
        (true, true)
    );
    assert!(!render_bottom_popup(&replacement, /*width*/ 70).contains("Set up security"));

    replacement.invalidate_security_setup();
    replacement.show_security_setup(setup_identity(), setup_notice());
    assert!(!render_bottom_popup(&replacement, /*width*/ 70).contains("Set up security"));

    // A verified change of account starts a new notice occurrence.
    replacement.invalidate_security_setup();
    replacement.show_security_setup(
        Identity {
            account: "other-account".into(),
            ..setup_identity()
        },
        setup_notice(),
    );
    assert!(render_bottom_popup(&replacement, /*width*/ 70).contains("Set up security"));
}

#[tokio::test]
async fn security_setup_after_cutoff_wraps_in_narrow_terminal() {
    let (mut chat, _events, _ops) = make_chatwidget_manual(Some("test-model-a")).await;
    let mut notice = setup_notice();
    notice.title = "Set up security for Daybreak mode".into();
    notice.description = "Set up Advanced Account Security with a hardware security key. Already Persona-verified? Add your key before October 15 to skip re-verification. You can keep using Lemex while you finish setup.".into();
    chat.show_security_setup(setup_identity(), notice);
    insta::assert_snapshot!(
        "security_setup_after_cutoff",
        normalize_snapshot_paths(render_bottom_popup(&chat, /*width*/ 48))
    );
}

#[tokio::test]
async fn security_setup_after_persona_grace_wraps_in_narrow_terminal() {
    let (mut chat, _events, _ops) = make_chatwidget_manual(Some("test-model-a")).await;
    let mut notice = setup_notice();
    notice.title = "Set up security for Daybreak mode".into();
    notice.description = "Set up Advanced Account Security with a hardware security key. Verify each new hardware security key with Persona. You can keep using Lemex while you finish setup.".into();
    chat.show_security_setup(setup_identity(), notice);
    insta::assert_snapshot!(
        "security_setup_after_persona_grace",
        normalize_snapshot_paths(render_bottom_popup(&chat, /*width*/ 48))
    );
}
