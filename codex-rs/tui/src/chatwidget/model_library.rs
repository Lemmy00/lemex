//! Curated saved models and searchable discovery share one live provider inventory.

use super::model_popups::MODEL_SELECTION_VIEW_ID;
use super::*;
use codex_model_provider::bundled_rcp_model_catalog;

pub(super) const ADD_MODELS_SELECTION_VIEW_ID: &str = "add-models-selection";

fn model_name(preset: &ModelPreset) -> String {
    if preset.display_name == preset.model {
        preset
            .model
            .rsplit('/')
            .next()
            .unwrap_or(&preset.model)
            .replace(['-', '_'], " ")
    } else {
        preset.display_name.clone()
    }
}

impl ChatWidget {
    pub(crate) fn set_model_picker_models(&mut self, models: Vec<String>) {
        self.config.model_picker_models = Some(models);
    }

    pub(super) fn open_saved_models_popup(&mut self, available: Vec<ModelPreset>) {
        let saved = self.config.model_picker_models.clone().unwrap_or_default();
        let bundled = bundled_rcp_model_catalog();
        let mut model_ids = Vec::new();
        let mut items = Vec::new();
        for id in saved {
            if model_ids.contains(&id) {
                continue;
            }
            let listed = available.iter().find(|preset| preset.model == id);
            let preset = listed.cloned().unwrap_or_else(|| {
                bundled
                    .models
                    .iter()
                    .find(|model| model.slug == id)
                    .cloned()
                    .unwrap_or_else(|| codex_models_manager::model_info::model_info_from_slug(&id))
                    .into()
            });
            let direct = preset.supported_reasoning_efforts.len() <= 1
                && !preset
                    .supported_reasoning_efforts
                    .iter()
                    .any(|option| Self::is_advanced_reasoning_effort(&option.effort));
            let is_current = self.current_model() == id;
            let effort = if is_current {
                self.effective_reasoning_effort()
            } else if preset.supported_reasoning_efforts.is_empty() {
                None
            } else {
                Some(preset.default_reasoning_effort.clone())
            };
            let description = if listed.is_some() {
                format!(
                    "Available · {}",
                    effort
                        .as_ref()
                        .map_or("model default", ReasoningEffortConfig::as_str)
                )
            } else {
                "Not listed for this key".to_string()
            };
            let for_action = preset.clone();
            items.push(SelectionItem {
                name: model_name(&preset),
                description: Some(description),
                is_current,
                is_disabled: listed.is_none(),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenReasoningPopup {
                        model: for_action.clone(),
                    })
                })],
                dismiss_on_select: direct,
                dismiss_parent_on_child_accept: !direct,
                ..Default::default()
            });
            model_ids.push(id);
        }
        items.push(SelectionItem {
            name: "Add model…".to_string(),
            description: Some("Search models available to your key".to_string()),
            actions: vec![Box::new(|tx| tx.send(AppEvent::OpenAddModelsPopup))],
            dismiss_on_select: true,
            ..Default::default()
        });
        model_ids.push("__add_model__".to_string());
        self.show_model_selection_view(
            model_ids,
            SelectionViewParams {
                view_id: Some(MODEL_SELECTION_VIEW_ID),
                header: self.model_menu_header(
                    "Your models",
                    "Select a model, then choose its reasoning level.",
                ),
                max_visible_rows: 10,
                items,
                ..SelectionViewParams::picker()
            },
        );
    }

    pub(crate) fn open_add_models_popup(&mut self) {
        self.show_add_models_popup();
        let request_id = uuid::Uuid::new_v4();
        self.model_popup_request_id = Some(request_id);
        self.app_event_tx.send(AppEvent::FetchModels { request_id });
    }

    pub(super) fn show_add_models_popup(&mut self) {
        let saved = self
            .config
            .model_picker_models
            .as_deref()
            .unwrap_or_default();
        let mut model_ids = Vec::new();
        let mut items = Vec::new();
        for preset in self.model_catalog.try_list_models().unwrap_or_default() {
            let added = saved.contains(&preset.model);
            let efforts = if preset.supported_reasoning_efforts.is_empty() {
                "model default".to_string()
            } else {
                preset
                    .supported_reasoning_efforts
                    .iter()
                    .map(|option| option.effort.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let direct = preset.supported_reasoning_efforts.len() <= 1
                && !preset
                    .supported_reasoning_efforts
                    .iter()
                    .any(|option| Self::is_advanced_reasoning_effort(&option.effort));
            let name = model_name(&preset);
            let search_value = format!("{name} {} {}", preset.display_name, preset.model);
            let for_action = preset.clone();
            model_ids.push(preset.model.clone());
            items.push(SelectionItem {
                name,
                description: Some(if added {
                    "Already added".to_string()
                } else {
                    "Available to add".to_string()
                }),
                selected_description: Some(format!("{} · Reasoning: {efforts}", preset.model)),
                search_value: Some(search_value),
                is_current: self.current_model() == preset.model,
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenReasoningPopup {
                        model: for_action.clone(),
                    })
                })],
                dismiss_on_select: direct,
                dismiss_parent_on_child_accept: !direct,
                ..Default::default()
            });
        }
        self.show_model_selection_view(
            model_ids,
            SelectionViewParams {
                view_id: Some(ADD_MODELS_SELECTION_VIEW_ID),
                header: self.model_menu_header(
                    "Add a model",
                    "Search by model name or ID. Select to add and use it.",
                ),
                items,
                is_searchable: true,
                search_placeholder: Some("Type a model name or ID…".to_string()),
                on_cancel: Some(Box::new(|tx| tx.send(AppEvent::OpenModelPopup))),
                ..SelectionViewParams::picker()
            },
        );
    }
}
