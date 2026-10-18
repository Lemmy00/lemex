//! Saved model lists, live discovery, and persistent model/reasoning selection.

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use codex_core::config::Config;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_login::AuthManager;
use codex_model_provider::bundled_rcp_model_catalog;
use codex_model_provider::discover_model_catalog;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::ReasoningEffort;
use codex_utils_cli::CliConfigOverrides;
use serde_json::json;
use std::collections::HashSet;
use std::io::IsTerminal;
use std::io::Write;

#[derive(Debug, Parser)]
pub(crate) struct ModelsCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,
    /// Print models, availability, and reasoning settings as JSON.
    #[arg(long)]
    json: bool,
    /// Browse model candidates listed by your provider using this API key.
    #[arg(long)]
    all: bool,
    /// Search available models by name or ID (implies --all).
    #[arg(long, value_name = "QUERY")]
    search: Option<String>,
    /// Use configured metadata without contacting the endpoint.
    #[arg(long)]
    offline: bool,
    #[command(subcommand)]
    action: Option<ModelsAction>,
}

#[derive(Debug, clap::Subcommand)]
enum ModelsAction {
    /// Add an available model to your saved list without switching models.
    Add { model: String },
    /// Remove a model from your saved list.
    Remove { model: String },
    /// Replace your shortlist, or choose it interactively when no IDs are given.
    Shortlist {
        models: Vec<String>,
        /// Empty the shortlist without changing the default model.
        #[arg(long, conflicts_with = "models")]
        clear: bool,
    },
    /// Restore the default model list.
    Reset,
}

#[derive(Debug, Parser)]
pub(crate) struct ModelCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,
    /// Model ID to save, or omit to choose interactively.
    model: Option<String>,
    /// A supported effort, or 'default' to clear the previous override.
    #[arg(long, value_name = "EFFORT")]
    reasoning: Option<String>,
    /// Choose using configured metadata without a network request.
    #[arg(long)]
    offline: bool,
}

struct ModelRow {
    model: ModelInfo,
    available: Option<bool>,
    added: bool,
}

async fn load(overrides: &CliConfigOverrides) -> anyhow::Result<Config> {
    Ok(
        Config::load_with_cli_overrides(overrides.parse_overrides().map_err(anyhow::Error::msg)?)
            .await?,
    )
}

async fn catalog(config: &Config, offline: bool) -> anyhow::Result<ModelsResponse> {
    if offline {
        return config.model_catalog.clone().context(
            "No offline model catalog is configured. Run `lemex models` to query the endpoint.",
        );
    }
    let auth = AuthManager::shared_from_config(config, true).await?;
    discover_model_catalog(config.model_provider.clone(), Some(auth), config.model_catalog.clone(), config.http_client_factory())
        .await.context("Could not fetch models from your endpoint. Check `lemex setup` and try again, or use --offline.")
}

fn saved_ids(config: &Config, catalog: &ModelsResponse) -> Vec<String> {
    let mut ids = config.model_picker_models.clone().unwrap_or_else(|| {
        catalog
            .models
            .iter()
            .map(|model| model.slug.clone())
            .collect()
    });
    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    ids
}

fn model_rows(
    config: &Config,
    catalog: &ModelsResponse,
    all: bool,
    offline: bool,
) -> Vec<ModelRow> {
    let saved = saved_ids(config, catalog);
    if all {
        return catalog
            .models
            .iter()
            .map(|model| ModelRow {
                model: model.clone(),
                available: (!offline).then_some(true),
                added: saved.contains(&model.slug),
            })
            .collect();
    }
    let bundled = if config.model_provider.is_rcp() {
        bundled_rcp_model_catalog()
    } else {
        ModelsResponse::default()
    };
    saved
        .into_iter()
        .map(|id| {
            let available = catalog.models.iter().find(|model| model.slug == id);
            let model = available
                .or_else(|| bundled.models.iter().find(|model| model.slug == id))
                .or_else(|| {
                    config
                        .model_catalog
                        .as_ref()
                        .and_then(|catalog| catalog.models.iter().find(|model| model.slug == id))
                })
                .cloned()
                .unwrap_or_else(|| codex_models_manager::model_info::model_info_from_slug(&id));
            ModelRow {
                model,
                available: (!offline).then_some(available.is_some()),
                added: true,
            }
        })
        .collect()
}

fn matches_query(model: &ModelInfo, query: &str) -> bool {
    if query.trim() == "*" {
        return true;
    }
    let text = format!("{} {}", model.display_name, model.slug).to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|part| text.contains(part))
}

pub(crate) async fn list(command: ModelsCommand) -> anyhow::Result<()> {
    let config = load(&command.config_overrides).await?;
    match command.action {
        Some(ModelsAction::Reset) => {
            ConfigEditsBuilder::for_config(&config)
                .set_model_picker_models(None)
                .apply()
                .await?;
            eprintln!("Restored the default model list.");
            return Ok(());
        }
        Some(ModelsAction::Remove { model }) => {
            let empty = config.model_catalog.clone().unwrap_or_default();
            let mut ids = saved_ids(&config, &empty);
            let before = ids.len();
            ids.retain(|id| id != &model);
            if ids.len() == before {
                bail!("Model '{model}' is not in your saved list.");
            }
            ConfigEditsBuilder::for_config(&config)
                .set_model_picker_models(Some(&ids))
                .apply()
                .await?;
            eprintln!("Removed {model} from your model list.");
            return Ok(());
        }
        Some(ModelsAction::Shortlist { clear: true, .. }) => {
            ConfigEditsBuilder::for_config(&config)
                .set_model_picker_models(Some(&[]))
                .apply()
                .await?;
            eprintln!("Shortlist cleared. Your default model is unchanged.");
            return Ok(());
        }
        _ => {}
    }
    let catalog = catalog(&config, command.offline).await?;
    if let Some(ModelsAction::Shortlist { models, .. }) = command.action {
        let models = if models.is_empty() {
            if !std::io::stdin().is_terminal() {
                bail!(
                    "Provide model IDs, use --clear, or run `lemex models shortlist` in a terminal."
                );
            }
            choose_shortlist(&config, &catalog)?
        } else {
            validated_shortlist(&catalog, models)?
        };
        ConfigEditsBuilder::for_config(&config)
            .set_model_picker_models(Some(&models))
            .apply()
            .await?;
        eprintln!(
            "Saved {} shortlisted models. Your default model is unchanged.",
            models.len()
        );
        return Ok(());
    }
    if let Some(ModelsAction::Add { model }) = command.action {
        find_model(&catalog, &model)?;
        let mut ids = saved_ids(&config, &catalog);
        if !ids.contains(&model) {
            ids.push(model.clone());
        }
        ConfigEditsBuilder::for_config(&config)
            .set_model_picker_models(Some(&ids))
            .apply()
            .await?;
        eprintln!("Added {model}. Choose it with `lemex model` or /model.");
        return Ok(());
    }
    let all = command.all || command.search.is_some();
    let rows = model_rows(&config, &catalog, all, command.offline)
        .into_iter()
        .filter(|row| {
            command
                .search
                .as_ref()
                .is_none_or(|query| matches_query(&row.model, query))
        })
        .collect::<Vec<_>>();
    if command.json {
        let models = rows.iter().map(|row| json!({
            "id":row.model.slug, "name":row.model.display_name, "description":row.model.description,
            "available":row.available, "added":row.added,
            "reasoning_efforts":row.model.supported_reasoning_levels,
            "default_reasoning_effort":row.model.default_reasoning_level,
        })).collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "source":if command.offline {"configured"} else {"live"},
                "inference_verified": false,
                "current_model":config.model, "models":models,
            }))?
        );
    } else {
        println!(
            "{} · {}\n",
            if all {
                "Provider models"
            } else {
                "Your models"
            },
            config.model_provider.name
        );
        for row in &rows {
            let marker = if config.model.as_deref() == Some(&row.model.slug) {
                "*"
            } else {
                " "
            };
            let status = match row.available {
                None => "offline",
                Some(false) => "not listed for this key",
                Some(true) if all && row.added => "listed · shortlisted",
                Some(true) => "listed",
            };
            println!("{marker} {}  · {status}", row.model.display_name);
            println!("    {}", row.model.slug);
            println!("    Reasoning: {}\n", effort_labels(&row.model));
        }
        if rows.is_empty() {
            println!("No matching models.\n");
        }
        println!(
            "Shortlist: lemex models shortlist\nDefault:   lemex model\nSearch:    lemex models --search QUERY\nAdd:       lemex models add MODEL_ID"
        );
        println!(
            "\nListed models are candidates; inference and tool calling have not been checked."
        );
    }
    Ok(())
}

pub(crate) async fn select(command: ModelCommand) -> anyhow::Result<()> {
    let config = load(&command.config_overrides).await?;
    select_with_config(command, config, None).await
}

async fn select_with_config(
    command: ModelCommand,
    config: Config,
    inventory: Option<ModelsResponse>,
) -> anyhow::Result<()> {
    if command.model.is_none() && command.reasoning.is_none() && !std::io::stdin().is_terminal() {
        println!(
            "Model: {}",
            config.model.as_deref().unwrap_or("not selected")
        );
        println!(
            "Reasoning: {}",
            config
                .model_reasoning_effort
                .as_ref()
                .map_or("model default", ReasoningEffort::as_str)
        );
        return Ok(());
    }
    let catalog = match inventory {
        Some(catalog) => catalog,
        None => catalog(&config, command.offline).await?,
    };
    if catalog.models.is_empty() {
        bail!("The endpoint listed no compatible model candidates for this key.");
    }
    let model = match command.model.as_deref() {
        Some(id) => find_model(&catalog, id)?.clone(),
        None if command.reasoning.is_some() => find_model(
            &catalog,
            config
                .model
                .as_deref()
                .context("No default model is saved. Choose one with `lemex model MODEL`.")?,
        )?
        .clone(),
        None => choose_model(&config, &catalog, command.offline)?,
    };
    let effort = if command.model.is_none() && command.reasoning.is_none() {
        choose_effort(&model)?
    } else {
        resolve_effort(&model, command.reasoning.as_deref())?
    };
    let mut edits =
        ConfigEditsBuilder::for_config(&config).set_model(Some(&model.slug), effort.clone());
    if config.model_picker_models.is_some() {
        let mut ids = saved_ids(&config, &catalog);
        if !ids.contains(&model.slug) {
            ids.push(model.slug.clone());
            edits = edits.set_model_picker_models(Some(&ids));
        }
    }
    edits.apply().await?;
    eprintln!("Default model: {}", model.slug);
    eprintln!(
        "Reasoning: {}",
        effort
            .as_ref()
            .map_or("model default", ReasoningEffort::as_str)
    );
    eprintln!("Run `lemex` to start. Use /model to switch within a session.");
    Ok(())
}

pub(crate) async fn select_interactively(
    overrides: &CliConfigOverrides,
    api_key: &str,
) -> anyhow::Result<()> {
    let mut config = load(overrides).await?;
    // Setup must discover models with the key it saved, even if the parent
    // terminal still exports another key. This override is never persisted.
    config.model_provider.env_key = None;
    config.model_provider.experimental_bearer_token = Some(api_key.to_string().into());
    let inventory = catalog(&config, false).await?;
    let shortlist = choose_shortlist(&config, &inventory)?;
    ConfigEditsBuilder::for_config(&config)
        .set_model_picker_models(Some(&shortlist))
        .apply()
        .await?;
    config.model_picker_models = Some(shortlist);
    select_with_config(
        ModelCommand {
            config_overrides: overrides.clone(),
            model: None,
            reasoning: None,
            offline: false,
        },
        config,
        Some(inventory),
    )
    .await
}

fn find_model<'a>(catalog: &'a ModelsResponse, id: &str) -> anyhow::Result<&'a ModelInfo> {
    catalog.models.iter().find(|model| model.slug == id)
        .with_context(|| format!("Model '{id}' is not listed by the selected endpoint. Run `lemex models --all` to see current IDs."))
}

fn validated_shortlist(
    catalog: &ModelsResponse,
    models: Vec<String>,
) -> anyhow::Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for id in models {
        find_model(catalog, &id)?;
        if seen.insert(id.clone()) {
            selected.push(id);
        }
    }
    Ok(selected)
}

fn choose_shortlist(config: &Config, catalog: &ModelsResponse) -> anyhow::Result<Vec<String>> {
    let mut selected = saved_ids(config, catalog);
    eprintln!(
        "\nChoose your shortlist. Adding or removing models here does not change the default."
    );
    eprintln!("Listed models are candidates; inference and tool calling have not been checked.");
    loop {
        eprintln!("\nShortlist ({}): {}", selected.len(), selected.join(", "));
        let query = read_choice("Search models ('*' to browse, Enter to save): ")?;
        if query.is_empty() {
            return Ok(selected);
        }
        let matches = catalog
            .models
            .iter()
            .filter(|model| matches_query(model, &query))
            .take(20)
            .collect::<Vec<_>>();
        if matches.is_empty() {
            eprintln!("No listed models match '{query}'.");
            continue;
        }
        for (index, model) in matches.iter().enumerate() {
            let marker = if selected.contains(&model.slug) {
                "x"
            } else {
                " "
            };
            eprintln!(
                "  {}. [{marker}] {}\n     {} · Reasoning: {}",
                index + 1,
                model.display_name,
                model.slug,
                effort_labels(model)
            );
        }
        let choice =
            read_choice("Toggle model numbers, separated by commas (Enter to search again): ")?;
        if choice.is_empty() {
            continue;
        }
        let choices = choice
            .split(',')
            .map(|value| {
                value
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| index.checked_sub(1))
                    .and_then(|index| matches.get(index).copied())
            })
            .collect::<Option<Vec<_>>>();
        let Some(choices) = choices else {
            eprintln!("Choose numbers from the displayed results.");
            continue;
        };
        let mut toggled = HashSet::new();
        for model in choices {
            if !toggled.insert(&model.slug) {
                continue;
            }
            if selected.contains(&model.slug) {
                selected.retain(|id| id != &model.slug);
            } else {
                selected.push(model.slug.clone());
            }
        }
    }
}

fn effort_labels(model: &ModelInfo) -> String {
    if model.supported_reasoning_levels.is_empty() {
        return "model default".to_string();
    }
    model
        .supported_reasoning_levels
        .iter()
        .map(|option| option.effort.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn resolve_effort(
    model: &ModelInfo,
    value: Option<&str>,
) -> anyhow::Result<Option<ReasoningEffort>> {
    let Some(value) = value.filter(|value| *value != "default") else {
        return Ok(None);
    };
    let effort: ReasoningEffort = value.parse().map_err(anyhow::Error::msg)?;
    if !model
        .supported_reasoning_levels
        .iter()
        .any(|option| option.effort == effort)
    {
        bail!(
            "Reasoning '{value}' is not supported for {}. Choose {} or default.",
            model.slug,
            effort_labels(model)
        );
    }
    Ok(Some(effort))
}

fn read_choice(prompt: &str) -> anyhow::Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut value = String::new();
    if std::io::stdin().read_line(&mut value)? == 0 {
        bail!("Model selection cancelled.")
    }
    Ok(value.trim().to_string())
}

fn search_model(catalog: &ModelsResponse) -> anyhow::Result<Option<ModelInfo>> {
    loop {
        let query = read_choice("Search model name or ID (Enter to go back): ")?;
        if query.is_empty() {
            return Ok(None);
        }
        let matches = catalog
            .models
            .iter()
            .filter(|model| matches_query(model, &query))
            .collect::<Vec<_>>();
        if matches.is_empty() {
            eprintln!("No listed models match '{query}'.");
            continue;
        }
        for (index, model) in matches.iter().take(12).enumerate() {
            eprintln!(
                "  {}. {}\n     {}",
                index + 1,
                model.display_name,
                model.slug
            );
        }
        if matches.len() > 12 {
            eprintln!(
                "{} matches; showing 12. Enter a narrower search to see more.",
                matches.len()
            );
        }
        let choice = read_choice("Model number or ID (Enter to search again): ")?;
        if let Ok(index) = choice.parse::<usize>()
            && let Some(model) = index
                .checked_sub(1)
                .and_then(|index| matches.iter().take(12).nth(index))
        {
            return Ok(Some((*model).clone()));
        }
        if let Ok(model) = find_model(catalog, &choice) {
            return Ok(Some(model.clone()));
        }
    }
}

fn choose_model(
    config: &Config,
    catalog: &ModelsResponse,
    offline: bool,
) -> anyhow::Result<ModelInfo> {
    let rows = model_rows(config, catalog, false, offline);
    let default = rows
        .iter()
        .position(|row| {
            Some(row.model.slug.as_str()) == config.model.as_deref() && row.available != Some(false)
        })
        .or_else(|| rows.iter().position(|row| row.available != Some(false)));
    eprintln!("\nYour models:");
    for (index, row) in rows.iter().enumerate() {
        let status = if row.available == Some(false) {
            "not listed for this key"
        } else {
            "listed"
        };
        eprintln!("  {}. {}  · {status}", index + 1, row.model.display_name);
    }
    eprintln!("  a. Search and add a listed model");
    loop {
        let default_label =
            default.map_or_else(|| "a".to_string(), |index| (index + 1).to_string());
        let choice = read_choice(&format!("Model number or ID [{default_label}]: "))?;
        let choice = if choice.is_empty() {
            default_label
        } else {
            choice
        };
        if choice == "a" || choice == "add" {
            if let Some(model) = search_model(catalog)? {
                return Ok(model);
            }
        } else if let Ok(index) = choice.parse::<usize>() {
            if let Some(row) = index.checked_sub(1).and_then(|index| rows.get(index)) {
                if row.available != Some(false) {
                    return Ok(row.model.clone());
                }
                eprintln!(
                    "That model is not listed for this key. Choose another model or search with 'a'."
                );
            }
        } else if let Ok(model) = find_model(catalog, &choice) {
            return Ok(model.clone());
        }
    }
}

fn choose_effort(model: &ModelInfo) -> anyhow::Result<Option<ReasoningEffort>> {
    if model.supported_reasoning_levels.is_empty() {
        eprintln!("Using the model's default reasoning; no explicit effort values are advertised.");
        return Ok(None);
    }
    loop {
        let value = read_choice(&format!(
            "Reasoning ({}, default) [default]: ",
            effort_labels(model)
        ))?;
        match resolve_effort(model, if value.is_empty() { None } else { Some(&value) }) {
            Ok(effort) => return Ok(effort),
            Err(error) => eprintln!("{error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_switch_does_not_carry_an_old_reasoning_override() {
        let mut model = codex_models_manager::model_info::model_info_from_slug("test");
        model.default_reasoning_level = Some(ReasoningEffort::High);
        assert_eq!(resolve_effort(&model, None).unwrap(), None);
        assert_eq!(resolve_effort(&model, Some("default")).unwrap(), None);
        assert!(resolve_effort(&model, Some("medium")).is_err());
    }
}
