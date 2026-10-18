//! Model-family presentation at constrained sizes and with configured list bindings.

use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn model_without_effort_metadata_clears_a_previous_override() {
    let (mut chat, mut events, _ops) = make_chatwidget_manual(Some("gpt-5.5")).await;
    let mut preset = get_available_model(&chat, "gpt-5.5");
    preset.id = "rcp/future-model".into();
    preset.model = preset.id.clone();
    preset.supported_reasoning_efforts.clear();
    chat.set_reasoning_effort(Some(ReasoningEffortConfig::High));
    chat.open_reasoning_popup(preset);
    assert_matches!(events.try_recv(), Ok(AppEvent::UpdateModel(model)) if model == "rcp/future-model");
    assert_matches!(events.try_recv(), Ok(AppEvent::UpdateReasoningEffort(None)));
    assert_matches!(
        events.try_recv(),
        Ok(AppEvent::PersistModelSelection { effort: None, .. })
    );
}

#[tokio::test]
async fn model_picker_compact_hint_and_actions_follow_configured_bindings() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.5")).await;
    let mut keymap = crate::keymap::RuntimeKeymap::defaults();
    keymap.list.accept = vec![key_hint::plain(KeyCode::F(3))];
    keymap.list.cancel = vec![key_hint::plain(KeyCode::F(2))];
    chat.bottom_pane.set_keymap_bindings(&keymap);
    chat.open_all_models_popup();
    let popup = render_bottom_popup(&chat, /*width*/ 40);
    assert_eq!(popup.lines().last().unwrap().trim(), "f3 select · f2 back");
    chat.handle_key_event(KeyEvent::from(KeyCode::F(3)));
    assert_matches!(rx.try_recv(), Ok(AppEvent::OpenReasoningPopup { .. }));
    chat.handle_key_event(KeyEvent::from(KeyCode::F(2)));
    assert!(chat.no_modal_or_popup_active());
}
