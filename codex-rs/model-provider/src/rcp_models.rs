//! RCP exposes OpenAI model IDs and separate LiteLLM deployment metadata.
//! The list of IDs determines availability; metadata enriches that list only.

use codex_models_manager::model_info::model_info_from_slug;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelVisibility;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) fn bundled_catalog() -> ModelsResponse {
    serde_json::from_str(include_str!("../../../config/models.json")).unwrap_or_else(|error| {
        tracing::error!(%error, "failed to load bundled RCP catalog");
        ModelsResponse::default()
    })
}

pub(crate) fn decode_catalog(
    body: &[u8],
    metadata: Option<&Value>,
    seed: &ModelsResponse,
) -> Result<Vec<ModelInfo>, String> {
    let response: Value = serde_json::from_slice(body)
        .map_err(|_| "RCP returned an invalid model list".to_string())?;
    let rows = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "RCP model list must contain a data array".to_string())?;
    let details = metadata
        .and_then(|value| value.get("data"))
        .and_then(Value::as_array);
    let profiles = bundled_catalog();
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for row in rows {
        let Some(id) = row.get("id").and_then(Value::as_str) else {
            continue;
        };
        if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) || !seen.insert(id) {
            continue;
        }
        let deployment = details.and_then(|rows| {
            rows.iter()
                .find(|row| row.get("model_name").and_then(Value::as_str) == Some(id))
        });
        let info = deployment.and_then(|row| row.get("model_info"));
        let deployed_model = deployment
            .and_then(|row| row.get("litellm_params"))
            .and_then(|params| params.get("model"))
            .and_then(Value::as_str)
            .map(|model| model.strip_prefix("hosted_vllm/").unwrap_or(model));
        if let Some(mode) = info
            .and_then(|value| value.get("mode"))
            .and_then(Value::as_str)
            && !matches!(mode, "chat" | "completion")
        {
            continue;
        }
        // The public list may include embeddings, rerankers, and transcription
        // models even when deployment metadata is temporarily unavailable.
        if info.is_none()
            && ["embedding", "reranker", "rerank-", "whisper"]
                .iter()
                .any(|part| id.to_lowercase().contains(part))
        {
            continue;
        }
        let known = seed
            .models
            .iter()
            .find(|model| model.slug == id)
            .or_else(|| {
                deployed_model.and_then(|deployed| {
                    seed.models
                        .iter()
                        .find(|model| deployed.starts_with(&model.slug))
                })
            });
        let mut model = known.cloned().unwrap_or_else(|| {
            let mut model = model_info_from_slug(id);
            model.context_window = None;
            model.max_context_window = None;
            if let Some(messages) = seed
                .models
                .first()
                .and_then(|model| model.model_messages.clone())
            {
                model.model_messages = Some(messages);
            }
            model
        });
        if model.slug != id {
            model.slug = id.to_string();
            model.display_name = id.to_string();
        }
        model.visibility = ModelVisibility::List;
        model.supported_in_api = true;
        model.used_fallback_model_metadata = false;
        model.supports_reasoning_summary_parameter = false;
        model.default_reasoning_summary = ReasoningSummary::None;
        model.supports_search_tool = false;
        model.support_verbosity = false;
        model.default_verbosity = None;
        model.priority = known.map_or(100, |model| model.priority);
        if let Some(context) = info
            .and_then(|value| value.get("max_input_tokens"))
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
        {
            model.context_window = Some(context);
            model.max_context_window = Some(context);
        }
        apply_reasoning(&mut model, info, deployed_model, &profiles);
        if info
            .and_then(|value| value.get("rcp_portal_labels"))
            .and_then(Value::as_array)
            .is_some_and(|labels| labels.iter().any(|label| label.as_str() == Some("eol")))
        {
            let description = model
                .description
                .get_or_insert_with(|| "RCP model".to_string());
            description.push_str(" (RCP marks this model as end of life)");
            model.priority += 1000;
        }
        models.push(model);
    }
    if models.is_empty() {
        return Err("RCP returned no text-generation models for this API key".to_string());
    }
    models.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.slug.cmp(&b.slug))
    });
    Ok(models)
}

fn apply_reasoning(
    model: &mut ModelInfo,
    info: Option<&Value>,
    deployed_model: Option<&str>,
    profiles: &ModelsResponse,
) {
    if let Some(values) = info
        .and_then(|info| info.get("supported_reasoning_efforts"))
        .and_then(Value::as_array)
    {
        model.supported_reasoning_levels = values
            .iter()
            .filter_map(|value| {
                let effort = value
                    .as_str()
                    .or_else(|| value.get("effort").and_then(Value::as_str))?;
                Some(ReasoningEffortPreset {
                    effort: effort.parse().ok()?,
                    description: format!("Provider-supported effort: {effort}"),
                })
            })
            .collect();
        model.default_reasoning_level = info
            .and_then(|info| info.get("default_reasoning_effort"))
            .and_then(Value::as_str)
            .and_then(|value| value.parse().ok())
            .filter(|effort| {
                model
                    .supported_reasoning_levels
                    .iter()
                    .any(|option| &option.effort == effort)
            });
        return;
    }
    // HTTP acceptance alone does not establish a distinct reasoning level:
    // GLM silently maps unknown values to max. Use the verified bundled profiles,
    // matching the most specific dated/quantized deployment before a base model.
    let family = deployed_model.unwrap_or(&model.slug);
    model.supported_reasoning_levels.clear();
    model.default_reasoning_level = None;
    if let Some(profile) = profiles
        .models
        .iter()
        .filter(|profile| {
            family == profile.slug
                || family
                    .strip_prefix(&profile.slug)
                    .is_some_and(|suffix| suffix.starts_with('-'))
        })
        .max_by_key(|profile| profile.slug.len())
    {
        model
            .supported_reasoning_levels
            .clone_from(&profile.supported_reasoning_levels);
        model
            .default_reasoning_level
            .clone_from(&profile.default_reasoning_level);
    } else if family.starts_with("openai/gpt-oss-") {
        // RCP's Harmony backend explicitly rejects other effort strings.
        model.supported_reasoning_levels = [
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
        ]
        .into_iter()
        .map(|effort| ReasoningEffortPreset {
            description: format!("RCP-supported effort: {effort}"),
            effort,
        })
        .collect();
        model.default_reasoning_level = Some(ReasoningEffort::Medium);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn live_ids_exclude_removed_models_and_non_chat_deployments() {
        let metadata = json!({"data":[{"model_name":"Qwen/Qwen3-Embedding-8B","model_info":{"mode":"embedding"}}]});
        let body = serde_json::to_vec(
            &json!({"data":[{"id":"Qwen/Qwen3.5-397B-A17B"},{"id":"Qwen/Qwen3-Embedding-8B"}]}),
        )
        .unwrap();
        let models = decode_catalog(&body, Some(&metadata), &bundled_catalog()).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].slug, "Qwen/Qwen3.5-397B-A17B");
    }

    #[test]
    fn dated_deepseek_model_has_its_actual_efforts() {
        let body = br#"{"data":[{"id":"deepseek-ai/DeepSeek-V4.1-Flash-1017"}]}"#;
        let models = decode_catalog(body, None, &bundled_catalog()).unwrap();
        assert_eq!(
            models[0].default_reasoning_level,
            Some(ReasoningEffort::Max)
        );
        assert_eq!(
            models[0]
                .supported_reasoning_levels
                .iter()
                .map(|option| option.effort.as_str())
                .collect::<Vec<_>>(),
            ["none", "low", "high", "xhigh", "max"]
        );
        assert!(!models[0].supports_reasoning_summary_parameter);
    }

    #[test]
    fn aliases_follow_the_deployed_models_reasoning_rules() {
        let metadata = json!({"data":[{"model_name":"epfl-aiaas/small","litellm_params":{"model":"hosted_vllm/deepseek-ai/DeepSeek-V4.1-Flash"},"model_info":{"mode":"completion"}}]});
        let models = decode_catalog(
            br#"{"data":[{"id":"epfl-aiaas/small"}]}"#,
            Some(&metadata),
            &bundled_catalog(),
        )
        .unwrap();
        assert_eq!(
            models[0].default_reasoning_level,
            Some(ReasoningEffort::High)
        );
        assert!(
            !models[0]
                .supported_reasoning_levels
                .iter()
                .any(|option| option.effort == ReasoningEffort::Medium)
        );
    }

    #[test]
    fn unknown_model_uses_provider_defaults_instead_of_inventing_efforts() {
        let models = decode_catalog(
            br#"{"data":[{"id":"future/new-model"}]}"#,
            None,
            &bundled_catalog(),
        )
        .unwrap();
        assert!(models[0].supported_reasoning_levels.is_empty());
        assert!(models[0].default_reasoning_level.is_none());
        assert!(models[0].context_window.is_none());
    }

    #[test]
    fn verified_profiles_replace_stale_local_effort_assumptions() {
        let mut stale_seed = bundled_catalog();
        for model in &mut stale_seed.models {
            model.default_reasoning_level = Some(ReasoningEffort::Medium);
            model.supported_reasoning_levels = vec![ReasoningEffortPreset {
                effort: ReasoningEffort::Medium,
                description: "old assumption".into(),
            }];
        }
        let models = decode_catalog(
            br#"{"data":[{"id":"Qwen/Qwen3.8-27B"},{"id":"zai-org/GLM-5.3-Flash"},{"id":"Qwen/Qwen3.5-397B-A17B"}]}"#,
            None,
            &stale_seed,
        )
        .unwrap();
        for (id, expected) in [
            ("Qwen/Qwen3.8-27B", vec!["none", "low", "medium", "xhigh"]),
            ("zai-org/GLM-5.3-Flash", vec!["low", "high", "max"]),
            ("Qwen/Qwen3.5-397B-A17B", vec![]),
        ] {
            let model = models.iter().find(|model| model.slug == id).unwrap();
            assert_eq!(
                model
                    .supported_reasoning_levels
                    .iter()
                    .map(|level| level.effort.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn deployment_metadata_overrides_local_assumptions() {
        let metadata = json!({"data":[{"model_name":"openai/gpt-oss-120b","model_info":{"mode":"completion","supported_reasoning_efforts":["low","custom"],"default_reasoning_effort":"custom"}}]});
        let models = decode_catalog(
            br#"{"data":[{"id":"openai/gpt-oss-120b"}]}"#,
            Some(&metadata),
            &bundled_catalog(),
        )
        .unwrap();
        assert_eq!(
            models[0].default_reasoning_level.as_ref().unwrap().as_str(),
            "custom"
        );
        assert_eq!(models[0].supported_reasoning_levels.len(), 2);
    }
}
