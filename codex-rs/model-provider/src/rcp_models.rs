//! Normalize provider model lists without guessing deployment capabilities.
//! RCP can additionally enrich its list with LiteLLM deployment metadata.

use codex_models_manager::model_info::model_info_from_slug;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelVisibility;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use serde_json::Value;
use std::collections::HashSet;

/// Curated RCP models and verified metadata shipped with Lemex.
pub fn bundled_catalog() -> ModelsResponse {
    serde_json::from_str(include_str!("../../../config/models.json")).unwrap_or_else(|error| {
        tracing::error!(%error, "failed to load bundled RCP catalog");
        ModelsResponse::default()
    })
}

#[cfg(test)]
fn decode_catalog(
    body: &[u8],
    metadata: Option<&Value>,
    seed: &ModelsResponse,
) -> Result<Vec<ModelInfo>, String> {
    decode_provider_catalog(body, metadata, seed, true)
}

pub(crate) fn decode_provider_catalog(
    body: &[u8],
    metadata: Option<&Value>,
    seed: &ModelsResponse,
    rcp: bool,
) -> Result<Vec<ModelInfo>, String> {
    let response: Value = serde_json::from_slice(body)
        .map_err(|_| "Provider returned an invalid model list".to_string())?;
    // Preserve rich catalogs supplied by a provider, including its reasoning settings.
    if response.get("models").is_some() {
        let mut catalog: ModelsResponse = serde_json::from_value(response)
            .map_err(|_| "Provider returned an invalid model catalog".to_string())?;
        let mut seen = HashSet::new();
        if catalog
            .models
            .iter()
            .any(|model| !valid_model_id(&model.slug) || !seen.insert(model.slug.clone()))
        {
            return Err("model catalog must contain unique, non-empty model slugs".to_string());
        }
        for model in &mut catalog.models {
            normalize_reasoning_default(model);
        }
        return Ok(catalog.models);
    }
    let rows = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "Provider model list must contain a data or models array".to_string())?;
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
        if !valid_model_id(id) || !seen.insert(id) {
            continue;
        }
        let deployment = details.and_then(|rows| {
            rows.iter()
                .find(|row| row.get("model_name").and_then(Value::as_str) == Some(id))
        });
        // Inline fields remain useful when optional deployment metadata omits
        // them; explicit deployment fields take precedence when both exist.
        let mut combined_info = row.clone();
        for extra in [
            row.get("model_info"),
            deployment.and_then(|value| value.get("model_info")),
        ]
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        {
            if let Some(info) = combined_info.as_object_mut() {
                info.extend(extra.clone());
            }
        }
        let info = Some(&combined_info);
        let deployed_model = deployment
            .and_then(|row| row.get("litellm_params"))
            .and_then(|params| params.get("model"))
            .and_then(Value::as_str)
            .map(|model| model.strip_prefix("hosted_vllm/").unwrap_or(model));
        if !is_candidate(row, info, id) {
            continue;
        }
        let known = seed
            .models
            .iter()
            .find(|model| model.slug == id)
            .or_else(|| {
                rcp.then(|| profiles.models.iter().find(|model| model.slug == id))
                    .flatten()
            })
            .or_else(|| {
                deployed_model.and_then(|deployed| {
                    seed.models
                        .iter()
                        .chain(profiles.models.iter())
                        .filter(|model| deployed.starts_with(&model.slug))
                        .max_by_key(|model| model.slug.len())
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
        if let Some(name) = row.get("name").and_then(Value::as_str) {
            model.display_name = name.to_string();
        }
        if let Some(description) = row.get("description").and_then(Value::as_str) {
            model.description = Some(description.to_string());
        }
        if let Some(context) = info
            .and_then(|value| {
                [
                    "max_input_tokens",
                    "context_length",
                    "context_window",
                    "max_context_window",
                ]
                .iter()
                .find_map(|key| value.get(key))
            })
            .or_else(|| row.get("context_length"))
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
        {
            model.context_window = Some(context);
            model.max_context_window = Some(context);
        }
        if !rcp {
            // RCP's verified effort profiles describe RCP deployments, not every
            // provider that happens to expose the same model ID.
            model.supported_reasoning_levels.clear();
            model.default_reasoning_level = None;
        }
        apply_reasoning(&mut model, info, deployed_model, &profiles, rcp);
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
    models.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.slug.cmp(&b.slug))
    });
    Ok(models)
}

fn valid_model_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}

fn is_candidate(row: &Value, info: Option<&Value>, id: &str) -> bool {
    if let Some(mode) = info
        .and_then(|value| value.get("mode").or_else(|| value.get("task")))
        .and_then(Value::as_str)
        && matches!(
            mode,
            "embedding"
                | "embeddings"
                | "rerank"
                | "reranking"
                | "image_generation"
                | "image-generation"
                | "transcription"
                | "audio_transcription"
                | "speech"
        )
    {
        return false;
    }
    if info
        .and_then(|value| {
            value
                .get("supports_function_calling")
                .or_else(|| value.get("supports_tool_calling"))
        })
        .and_then(Value::as_bool)
        == Some(false)
    {
        return false;
    }
    if let Some(endpoints) = info
        .and_then(|value| value.get("supported_endpoints"))
        .and_then(Value::as_array)
        && !endpoints.is_empty()
        && !endpoints.iter().filter_map(Value::as_str).any(|endpoint| {
            endpoint.trim_end_matches('/').ends_with("responses") || endpoint == "responses.create"
        })
    {
        return false;
    }
    if let Some(parameters) = row.get("supported_parameters").and_then(Value::as_array)
        && !parameters.is_empty()
        && !parameters
            .iter()
            .any(|parameter| parameter.as_str() == Some("tools"))
    {
        return false;
    }
    if let Some(outputs) = row
        .pointer("/architecture/output_modalities")
        .and_then(Value::as_array)
        && !outputs.is_empty()
        && !outputs
            .iter()
            .any(|modality| modality.as_str() == Some("text"))
    {
        return false;
    }
    !["embedding", "reranker", "rerank-", "whisper"]
        .iter()
        .any(|part| id.to_lowercase().contains(part))
}

fn normalize_reasoning_default(model: &mut ModelInfo) {
    let mut seen = HashSet::new();
    model
        .supported_reasoning_levels
        .retain(|option| seen.insert(option.effort.as_str().to_string()));
    model.default_reasoning_level = model.default_reasoning_level.take().filter(|effort| {
        model
            .supported_reasoning_levels
            .iter()
            .any(|option| &option.effort == effort)
    });
}

fn apply_reasoning(
    model: &mut ModelInfo,
    info: Option<&Value>,
    deployed_model: Option<&str>,
    profiles: &ModelsResponse,
    rcp: bool,
) {
    if let Some(values) = info
        .and_then(|info| {
            info.get("supported_reasoning_efforts")
                .or_else(|| info.get("supported_reasoning_levels"))
                .or_else(|| info.get("reasoning_efforts"))
                .or_else(|| info.pointer("/reasoning/efforts"))
        })
        .and_then(Value::as_array)
    {
        model.supported_reasoning_levels = values
            .iter()
            .filter_map(|value| {
                let effort = value
                    .as_str()
                    .or_else(|| value.get("effort").and_then(Value::as_str))?;
                if effort.trim().is_empty()
                    || effort.len() > 64
                    || effort.chars().any(char::is_control)
                {
                    return None;
                }
                Some(ReasoningEffortPreset {
                    effort: effort.parse().ok()?,
                    description: format!("Provider-supported effort: {effort}"),
                })
            })
            .collect();
        model.default_reasoning_level = info
            .and_then(|info| {
                info.get("default_reasoning_effort")
                    .or_else(|| info.get("default_reasoning_level"))
                    .or_else(|| info.pointer("/reasoning/default_effort"))
            })
            .and_then(Value::as_str)
            .and_then(|value| value.parse().ok())
            .filter(|effort| {
                model
                    .supported_reasoning_levels
                    .iter()
                    .any(|option| &option.effort == effort)
            });
        normalize_reasoning_default(model);
        return;
    }
    if !rcp {
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

    fn generic_catalog(value: Value) -> Vec<ModelInfo> {
        decode_provider_catalog(
            &serde_json::to_vec(&value).unwrap(),
            None,
            &bundled_catalog(),
            false,
        )
        .unwrap()
    }

    #[test]
    fn generic_discovery_filters_explicitly_incompatible_models_and_deduplicates_ids() {
        let models = generic_catalog(json!({"data": [
            {"id": "provider/chat", "name": "Chat model", "context_length": 64000},
            {"id": "provider/chat"},
            {"id": "provider/image", "architecture": {"output_modalities": ["image"]}},
            {"id": "provider/embedding", "mode": "embedding"},
            {"id": "provider/no-tools", "supports_tool_calling": false},
            {"id": "provider/chat-only", "supported_endpoints": ["/v1/chat/completions"]},
            {"id": "provider/with-tools", "supported_parameters": ["tools", "reasoning"]},
            {"id": "provider/without-tools", "supported_parameters": ["temperature"]},
            {"id": " "}, {"id": "invalid\nmodel"}
        ]}));
        assert_eq!(
            models
                .iter()
                .map(|model| model.slug.as_str())
                .collect::<Vec<_>>(),
            ["provider/chat", "provider/with-tools"]
        );
        assert_eq!(models[0].display_name, "Chat model");
        assert_eq!(models[0].max_context_window, Some(64000));
    }

    #[test]
    fn other_providers_do_not_inherit_rcp_reasoning_from_a_matching_model_name() {
        let models = generic_catalog(json!({"data": [{"id": "Qwen/Qwen3.8-27B"}]}));
        assert!(models[0].supported_reasoning_levels.is_empty());
        assert!(models[0].default_reasoning_level.is_none());
    }

    #[test]
    fn generic_advertised_reasoning_overrides_profiles_and_rejects_invalid_defaults() {
        let models = generic_catalog(json!({"data": [
            {"id": "Qwen/Qwen3.8-27B", "supported_reasoning_efforts": ["low", "custom", "low"],
                "default_reasoning_effort": "custom"},
            {"id": "provider/unknown-default", "model_info": {
                "supported_reasoning_levels": [{"effort": "none"}, {"effort": "high"}],
                "default_reasoning_level": "max"}},
            {"id": "provider/no-values", "supported_parameters": ["tools", "reasoning"]}
        ]}));
        let qwen = models
            .iter()
            .find(|model| model.slug.starts_with("Qwen/"))
            .unwrap();
        assert_eq!(
            qwen.default_reasoning_level.as_ref().unwrap().as_str(),
            "custom"
        );
        assert_eq!(qwen.supported_reasoning_levels.len(), 2);
        assert!(
            models
                .iter()
                .find(|model| model.slug.ends_with("unknown-default"))
                .unwrap()
                .default_reasoning_level
                .is_none()
        );
        assert!(
            models
                .iter()
                .find(|model| model.slug.ends_with("no-values"))
                .unwrap()
                .supported_reasoning_levels
                .is_empty()
        );
    }

    #[test]
    fn empty_provider_inventory_is_authoritative() {
        assert!(generic_catalog(json!({"data": []})).is_empty());
        assert!(generic_catalog(json!({"data": [{"id": "only-embedding"}]})).is_empty());
    }

    #[test]
    fn rich_catalogs_preserve_advertised_profiles() {
        let catalog = bundled_catalog();
        let models = generic_catalog(serde_json::to_value(&catalog).unwrap());
        assert_eq!(models, catalog.models);
    }

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
