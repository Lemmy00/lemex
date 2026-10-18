//! API-only setup. Secrets use the existing credential store, never config.toml.

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use codex_core::config::Config;
use codex_core::config::edit::ConfigEdit;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_core::config::find_codex_home;
use codex_login::login_with_api_key;
use codex_model_provider_info::DEFAULT_LEMEX_BASE_URL;
use codex_model_provider_info::LEMEX_API_KEY_ENV_VAR;
use codex_model_provider_info::LEMEX_BASE_URL_ENV_VAR;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use std::io::IsTerminal;
use std::io::Read;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Parser)]
pub(crate) struct SetupCommand {
    #[clap(skip)]
    pub config_overrides: codex_utils_cli::CliConfigOverrides,

    /// Read the API key from stdin instead of prompting.
    #[arg(long)]
    with_api_key: bool,

    /// Save a Responses API endpoint URL (default: RCP).
    #[arg(long, value_name = "URL")]
    base_url: Option<String>,

    /// Set the model ID for this endpoint.
    #[arg(long, value_name = "MODEL")]
    model: Option<String>,
}

pub(crate) async fn run(command: SetupCommand) -> anyhow::Result<()> {
    let home = find_codex_home()?;
    let interactive = std::io::stdin().is_terminal();
    if command.with_api_key && interactive {
        bail!("Pipe your API key to `lemex setup --with-api-key`.");
    }
    let overrides = command
        .config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    let existing_config = Config::load_with_cli_overrides(overrides).await?;
    if existing_config.cli_auth_credentials_store_mode
        == codex_config::types::AuthCredentialsStoreMode::Ephemeral
    {
        bail!(
            "Setup needs persistent credentials. Set cli_auth_credentials_store to file, auto, or keyring."
        );
    }
    if !existing_config
        .auth_config()
        .is_login_method_allowed(codex_protocol::config_types::ForcedLoginMethod::Api)
    {
        bail!(
            "API keys are disabled by your authentication policy. Update that policy before running setup."
        );
    }
    let stored = codex_login::load_auth_dot_json(
        &home,
        existing_config.cli_auth_credentials_store_mode,
        existing_config.auth_keyring_backend_kind(),
    )?;
    let saved_url = existing_config
        .model_providers
        .get(codex_model_provider_info::RCP_PROVIDER_ID)
        .and_then(|provider| provider.base_url.as_deref())
        .unwrap_or(DEFAULT_LEMEX_BASE_URL);
    let url = validate_url(command.base_url.as_deref().unwrap_or(saved_url))?;
    if interactive {
        eprintln!("API endpoint: {url}");
        eprintln!("To change the endpoint, use `lemex setup --base-url https://your-server/v1`.");
    }
    let environment_key = std::env::var(LEMEX_API_KEY_ENV_VAR)
        .ok()
        .filter(|key| !key.trim().is_empty());
    let has_saved_key = stored.as_ref().is_some_and(|auth| {
        auth.auth_mode
            .is_none_or(|mode| mode == codex_protocol::auth::AuthMode::ApiKey)
            && auth
                .openai_api_key
                .as_ref()
                .is_some_and(|key| !key.trim().is_empty())
    });
    let api_key = if command.with_api_key {
        let mut value = String::new();
        std::io::stdin().read_to_string(&mut value)?;
        Some(validate_key(&value)?)
    } else if interactive {
        read_hidden_key(has_saved_key || environment_key.is_some())?
            .or_else(|| environment_key.clone())
    } else if let Some(key) = environment_key.as_deref() {
        Some(validate_key(key)?)
    } else if has_saved_key {
        None
    } else {
        bail!("Run `lemex setup` in a terminal, or pipe a key to `lemex setup --with-api-key`.");
    };
    if command
        .model
        .as_ref()
        .is_some_and(|model| model.trim().is_empty())
    {
        bail!("Model ID must not be empty.");
    }

    install_defaults(&home)?;
    let mut edits = ConfigEditsBuilder::new(&home).with_edits([
        ConfigEdit::SetPath {
            segments: vec!["model_provider".into()],
            value: "rcp".into(),
        },
        ConfigEdit::SetPath {
            segments: vec!["model_providers".into(), "rcp".into(), "name".into()],
            value: "RCP".into(),
        },
        ConfigEdit::SetPath {
            segments: vec!["model_providers".into(), "rcp".into(), "base_url".into()],
            value: url.clone().into(),
        },
    ]);
    if let Some(model) = command.model.as_deref() {
        edits = edits.set_model(Some(model), None);
    }
    edits.apply().await?;
    if let Some(api_key) = api_key {
        login_with_api_key(
            &home,
            &api_key,
            existing_config.cli_auth_credentials_store_mode,
            existing_config.auth_keyring_backend_kind(),
        )
        .context("Could not save the API key")?;
    }
    eprintln!("API setup saved. Run `lemex` to start, or `lemex doctor` to check your setup.");
    if std::env::var(LEMEX_BASE_URL_ENV_VAR).is_ok_and(|value| !value.trim().is_empty()) {
        eprintln!("LEMEX_BASE_URL overrides the saved URL while it is set.");
    }
    if environment_key.is_some() {
        eprintln!("LEMEX_API_KEY overrides the saved key while it is set.");
    }
    if interactive && command.model.is_none() {
        if let Err(error) = crate::models::select_interactively(&command.config_overrides).await {
            eprintln!("Model selection could not complete: {error}");
            eprintln!(
                "Your API setup is saved. Run `lemex models` or `lemex model` to choose a model."
            );
        }
    }
    Ok(())
}

fn install_defaults(home: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(home)?;
    for (name, contents) in [
        ("config.toml", include_str!("../../../config/config.toml")),
        ("models.json", include_str!("../../../config/models.json")),
    ] {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(home.join(name))
        {
            Ok(mut file) => file.write_all(contents.as_bytes())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn validate_url(value: &str) -> anyhow::Result<String> {
    let url =
        url::Url::parse(value.trim()).context("API URL must be an absolute http or https URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("API URL must use http or https, without credentials, a query, or a fragment.");
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn validate_key(value: &str) -> anyhow::Result<String> {
    let key = value.trim();
    if key.starts_with("http://") || key.starts_with("https://") {
        bail!(
            "This looks like an API URL. Set the endpoint with --base-url and enter your API key at the hidden key prompt."
        );
    }
    if key.is_empty() || key.chars().any(char::is_whitespace) || key.chars().any(char::is_control) {
        bail!("Provide one non-empty API key.");
    }
    Ok(key.to_string())
}

fn read_hidden_key(can_keep_key: bool) -> anyhow::Result<Option<String>> {
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = crossterm::execute!(std::io::stderr(), crossterm::event::DisableBracketedPaste);
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
    crossterm::terminal::enable_raw_mode()?;
    let guard = TerminalGuard;
    crossterm::execute!(std::io::stderr(), crossterm::event::EnableBracketedPaste)?;
    if can_keep_key {
        eprint!("API key (hidden; Enter keeps the existing key): ");
    } else {
        eprint!("API key (hidden): ");
    }
    std::io::stderr().flush()?;
    let mut value = String::new();
    let result = loop {
        match crossterm::event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter if can_keep_key && value.trim().is_empty() => break Ok(None),
                KeyCode::Enter => break validate_key(&value).map(Some),
                KeyCode::Esc => break Err(anyhow::anyhow!("Setup cancelled.")),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break Err(anyhow::anyhow!("Setup cancelled."));
                }
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    value.push(ch)
                }
                _ => {}
            },
            Event::Paste(text) => value.push_str(text.trim()),
            _ => {}
        }
    };
    drop(guard);
    eprintln!();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_urls_and_credentials() {
        for url in [
            "",
            "example.com",
            "file:///tmp/key",
            "https://key@example.com/v1",
            "https://example.com/v1?key=secret",
            "https://example.com/v1#fragment",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
        assert_eq!(
            validate_url(" https://example.com/v1/ ").unwrap(),
            "https://example.com/v1"
        );
        assert_eq!(
            validate_url("http://localhost:8080/v1").unwrap(),
            "http://localhost:8080/v1"
        );
    }

    #[test]
    fn rejects_empty_or_multiple_keys() {
        assert_eq!(validate_key("test-key\n").unwrap(), "test-key");
        for key in [
            "",
            " \n",
            "key one",
            "one\ntwo",
            "key\0",
            "https://example.com/v1",
        ] {
            assert!(validate_key(key).is_err());
        }
    }

    #[test]
    fn defaults_preserve_existing_configuration() -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::write(home.path().join("config.toml"), "# existing config\n")?;
        install_defaults(home.path())?;
        assert_eq!(
            std::fs::read_to_string(home.path().join("config.toml"))?,
            "# existing config\n"
        );
        assert!(home.path().join("models.json").is_file());
        Ok(())
    }
}
