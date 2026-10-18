//! Model-family presentation at constrained sizes and with configured list bindings.

use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn curated_models_show_only_saved_models_and_key_availability() {
    let (mut chat, mut events, _ops) = make_chatwidget_manual(None).await;
    let models = codex_model_provider::bundled_rcp_model_catalog().models;
    chat.config.model_picker_models = Some(models.iter().map(|model| model.slug.clone()).collect());
    let mut available: Vec<ModelPreset> = models
        .into_iter()
        .filter(|model| model.slug != "mistralai/Leanstral-2603")
        .map(Into::into)
        .collect();
    let mut extra = available[0].clone();
    extra.id = "provider/extra-model".into();
    extra.model = extra.id.clone();
    extra.display_name = "Extra search model".into();
    available.push(extra);
    chat.set_model("deepseek-ai/DeepSeek-V4.1-Flash-1017");
    chat.set_reasoning_effort(Some(ReasoningEffortConfig::Max));
    chat.open_model_popup_with_presets(available);
    let popup = render_bottom_popup(&chat, 100);
    assert!(popup.contains("Your models"));
    assert!(popup.contains("Leanstral 2603"));
    assert!(popup.contains("Not listed for this key"));
    assert!(popup.contains("Add model…"));
    assert!(!popup.contains("Extra search model"));
    assert!(!popup.contains("via RCP"));
    assert_chatwidget_snapshot!("curated_models", popup);
    chat.handle_key_event(KeyEvent::from(KeyCode::End));
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));
    assert_matches!(events.try_recv(), Ok(AppEvent::OpenAddModelsPopup));
}

#[tokio::test]
async fn model_discovery_search_survives_refresh_and_shortlists_without_switching_model() {
    let (mut chat, mut events, _ops) = make_chatwidget_manual(None).await;
    let models = codex_model_provider::bundled_rcp_model_catalog().models;
    chat.config.model_picker_models = Some(models.iter().map(|model| model.slug.clone()).collect());
    let mut available: Vec<ModelPreset> = models.into_iter().map(Into::into).collect();
    let mut extra = available[0].clone();
    extra.id = "provider/extra-model".into();
    extra.model = extra.id.clone();
    extra.display_name = "Extra search model".into();
    extra.supported_reasoning_efforts.clear();
    available.push(extra.clone());
    Arc::make_mut(&mut chat.model_catalog).models = available.clone();
    chat.open_add_models_popup();
    let request = match events.try_recv().unwrap() {
        AppEvent::FetchModels { request_id } => request_id,
        event => panic!("expected catalog refresh, got {event:?}"),
    };
    for letter in "EXTRA".chars() {
        chat.handle_key_event(KeyEvent::from(KeyCode::Char(letter)));
    }
    let popup = render_bottom_popup(&chat, 100);
    assert!(popup.contains("Extra search model"));
    assert!(!popup.contains("DeepSeek V4.1 Flash"));
    let mut other = extra;
    other.id = "provider/other-model".into();
    other.model = other.id.clone();
    other.display_name = "Other model".into();
    available.push(other);
    assert!(chat.on_models_loaded(request, Ok(available)));
    let refreshed = render_bottom_popup(&chat, 100);
    assert!(refreshed.contains("Extra search model"));
    assert!(!refreshed.contains("Other model"));
    assert!(!refreshed.contains("DeepSeek V4.1 Flash"));
    assert_chatwidget_snapshot!("model_discovery_search", refreshed);
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));
    assert_matches!(events.try_recv(), Ok(AppEvent::ToggleModelShortlist { model }) if model == "provider/extra-model");
    assert!(events.try_recv().is_err());
    assert_ne!(chat.current_model(), "provider/extra-model");
    assert!(render_bottom_popup(&chat, 100).contains("Extra search model"));
}

#[tokio::test]
async fn model_discovery_refresh_populates_an_empty_search_and_escape_returns_to_saved_models() {
    let (mut chat, mut events, _ops) = make_chatwidget_manual(None).await;
    let models = codex_model_provider::bundled_rcp_model_catalog().models;
    chat.config.model_picker_models = Some(models.iter().map(|model| model.slug.clone()).collect());
    let mut available: Vec<ModelPreset> = models.into_iter().map(Into::into).collect();
    Arc::make_mut(&mut chat.model_catalog).models = available.clone();
    chat.open_add_models_popup();
    let request = match events.try_recv().unwrap() {
        AppEvent::FetchModels { request_id } => request_id,
        event => panic!("expected catalog refresh, got {event:?}"),
    };
    for letter in "future".chars() {
        chat.handle_key_event(KeyEvent::from(KeyCode::Char(letter)));
    }
    let mut extra = available[0].clone();
    extra.id = "provider/future-model".into();
    extra.model = extra.id.clone();
    extra.display_name = "Future model".into();
    available.push(extra);
    assert!(chat.on_models_loaded(request, Ok(available)));
    let popup = render_bottom_popup(&chat, 100);
    assert!(popup.contains("Future model"));
    assert!(!popup.contains("DeepSeek V4.1 Flash"));
    chat.handle_key_event(KeyEvent::from(KeyCode::Esc));
    assert!(chat.no_modal_or_popup_active());
    assert_matches!(events.try_recv(), Ok(AppEvent::OpenModelPopup));
    assert!(events.try_recv().is_err());
}

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
