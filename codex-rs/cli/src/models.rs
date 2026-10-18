//! User-facing discovery and persistent model/reasoning selection.

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use codex_core::config::Config;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_login::AuthManager;
use codex_model_provider::discover_model_catalog;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::ReasoningEffort;
use codex_utils_cli::CliConfigOverrides;
use serde_json::json;
use std::io::IsTerminal;
use std::io::Write;

#[derive(Debug, Parser)]
pub(crate) struct ModelsCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,
    /// Print model IDs and supported reasoning settings as JSON.
    #[arg(long)]
    json: bool,
    /// Use the configured catalog without contacting the endpoint.
    #[arg(long)]
    offline: bool,
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
    /// Choose from the configured catalog without a network request.
    #[arg(long)]
    offline: bool,
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

pub(crate) async fn list(command: ModelsCommand) -> anyhow::Result<()> {
    let config = load(&command.config_overrides).await?;
    let catalog = catalog(&config, command.offline).await?;
    if command.json {
        let models = catalog
            .models
            .iter()
            .map(|model| {
                json!({
                    "id":model.slug, "name":model.display_name, "description":model.description,
                    "reasoning_efforts":model.supported_reasoning_levels,
                    "default_reasoning_effort":model.default_reasoning_level,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "source":if command.offline {"configured"} else {"live"},
                "current_model":config.model, "models":models,
            }))?
        );
    } else {
        println!(
            "{} models for {}:",
            if command.offline {
                "Configured (offline)"
            } else {
                "Live"
            },
            config.model_provider.name
        );
        for model in &catalog.models {
            let current = if config.model.as_deref() == Some(model.slug.as_str()) {
                "*"
            } else {
                " "
            };
            println!(
                "{current} {}  [reasoning: {}]",
                model.slug,
                effort_labels(model)
            );
        }
        println!("\nChoose: lemex model MODEL --reasoning EFFORT");
        println!("Use --reasoning default to clear a saved override, or /model inside Lemex.");
    }
    Ok(())
}

pub(crate) async fn select(command: ModelCommand) -> anyhow::Result<()> {
    let config = load(&command.config_overrides).await?;
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
    let catalog = catalog(&config, command.offline).await?;
    let model = match command.model.as_deref() {
        Some(id) => find_model(&catalog, id)?,
        None if command.reasoning.is_some() => find_model(
            &catalog,
            config
                .model
                .as_deref()
                .context("No default model is saved. Choose one with `lemex model MODEL`.")?,
        )?,
        None => choose_model(&catalog, config.model.as_deref())?,
    };
    let effort = if command.model.is_none() && command.reasoning.is_none() {
        choose_effort(model)?
    } else {
        resolve_effort(model, command.reasoning.as_deref())?
    };
    save(&config, model, effort).await
}

pub(crate) async fn select_interactively(overrides: &CliConfigOverrides) -> anyhow::Result<()> {
    select(ModelCommand {
        config_overrides: overrides.clone(),
        model: None,
        reasoning: None,
        offline: false,
    })
    .await
}

async fn save(
    config: &Config,
    model: &ModelInfo,
    effort: Option<ReasoningEffort>,
) -> anyhow::Result<()> {
    ConfigEditsBuilder::for_config(config)
        .set_model(Some(&model.slug), effort.clone())
        .apply()
        .await?;
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

fn find_model<'a>(catalog: &'a ModelsResponse, id: &str) -> anyhow::Result<&'a ModelInfo> {
    catalog.models.iter().find(|model| model.slug == id)
        .with_context(|| format!("Model '{id}' is not listed by the selected endpoint. Run `lemex models` to see current IDs."))
}

fn effort_labels(model: &ModelInfo) -> String {
    if model.supported_reasoning_levels.is_empty() {
        return "model default; explicit efforts not advertised".to_string();
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

fn choose_model<'a>(
    catalog: &'a ModelsResponse,
    current: Option<&str>,
) -> anyhow::Result<&'a ModelInfo> {
    if catalog.models.is_empty() {
        bail!("The endpoint listed no models.")
    }
    eprintln!("\nModels listed by your endpoint:");
    for (index, model) in catalog.models.iter().enumerate() {
        eprintln!(
            "  {}. {}  [{}]",
            index + 1,
            model.slug,
            effort_labels(model)
        );
    }
    let default = current
        .and_then(|id| catalog.models.iter().position(|model| model.slug == id))
        .unwrap_or(0);
    if current.is_some_and(|id| !catalog.models.iter().any(|model| model.slug == id)) {
        eprintln!("Your previous model is no longer listed. Choose an available model below.");
    }
    loop {
        let choice = read_choice(&format!("Model number or ID [{}]: ", default + 1))?;
        if choice.is_empty() {
            return Ok(&catalog.models[default]);
        }
        if let Ok(index) = choice.parse::<usize>()
            && let Some(model) = index
                .checked_sub(1)
                .and_then(|index| catalog.models.get(index))
        {
            return Ok(model);
        }
        if let Ok(model) = find_model(catalog, &choice) {
            return Ok(model);
        }
        eprintln!("Choose a number or model ID from the list.");
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
