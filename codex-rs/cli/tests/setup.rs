//! Regression coverage for the API-only first-run path.

use anyhow::Result;
use predicates::str::contains;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

fn command(home: &Path) -> Result<assert_cmd::Command> {
    let mut command = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("lemex")?);
    command
        .current_dir(home)
        .env("LEMEX_HOME", home)
        .env_remove("LEMEX_API_KEY")
        .env_remove("LEMEX_BASE_URL")
        .env_remove("LEMEX_ACCESS_TOKEN")
        .env_remove("OPENAI_FEDERATION_RULE_ID")
        .env_remove("OPENAI_IDENTITY_TOKEN_FILE")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .timeout(std::time::Duration::from_secs(30));
    Ok(command)
}

#[test]
fn help_exposes_setup_and_removes_account_login() -> Result<()> {
    let home = TempDir::new()?;
    let output = command(home.path())?
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(output)?;
    assert!(help.contains("setup"));
    assert!(
        !help
            .lines()
            .any(|line| line.trim_start().starts_with("login ")
                || line.trim_start().starts_with("logout "))
    );
    for args in [
        vec!["login"],
        vec!["login", "--device-auth"],
        vec!["logout"],
    ] {
        command(home.path())?
            .args(args)
            .assert()
            .failure()
            .stderr(contains("Account login and logout have been removed"));
    }
    Ok(())
}

#[test]
fn setup_saves_key_and_rcp_defaults_without_exposing_secret() -> Result<()> {
    let home = TempDir::new()?;
    let output = command(home.path())?
        .args(["setup", "--with-api-key"])
        .write_stdin("test-setup-secret\n")
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("test-setup-secret"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("test-setup-secret"));
    let config = std::fs::read_to_string(home.path().join("config.toml"))?;
    let parsed: toml::Value = toml::from_str(&config)?;
    assert_eq!(parsed["model_provider"].as_str(), Some("rcp"));
    assert_eq!(
        parsed["model_providers"]["rcp"]["base_url"].as_str(),
        Some("https://inference.rcp.epfl.ch/v1")
    );
    assert!(!config.contains("test-setup-secret"));
    assert!(home.path().join("models.json").is_file());
    let auth: Value =
        serde_json::from_str(&std::fs::read_to_string(home.path().join("auth.json"))?)?;
    assert_eq!(auth["LEMEX_API_KEY"], "test-setup-secret");
    assert_eq!(auth["auth_mode"], "apikey");
    assert!(auth.get("tokens").is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(home.path().join("auth.json"))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    Ok(())
}

#[test]
fn setup_preserves_replacement_key_and_explains_a_different_environment_key() -> Result<()> {
    let home = TempDir::new()?;
    let output = command(home.path())?
        .env("LEMEX_API_KEY", "old-environment-secret")
        .args(["setup", "--with-api-key"])
        .write_stdin("replacement-secret\n")
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("different LEMEX_API_KEY"));
    assert!(stderr.contains(if cfg!(windows) {
        "Remove-Item Env:LEMEX_API_KEY"
    } else {
        "unset LEMEX_API_KEY"
    }));
    let config = std::fs::read_to_string(home.path().join("config.toml"))?;
    for secret in ["old-environment-secret", "replacement-secret"] {
        assert!(!stderr.contains(secret));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
        assert!(!config.contains(secret));
    }
    let auth: Value =
        serde_json::from_str(&std::fs::read_to_string(home.path().join("auth.json"))?)?;
    assert_eq!(auth["LEMEX_API_KEY"], "replacement-secret");
    Ok(())
}

#[test]
fn setup_updates_url_and_preserves_key_and_other_settings() -> Result<()> {
    let home = TempDir::new()?;
    std::fs::write(
        home.path().join("config.toml"),
        "# keep this comment\nmodel = 'existing-model'\nshow_raw_agent_reasoning = false\n",
    )?;
    command(home.path())?
        .args(["setup", "--with-api-key"])
        .write_stdin("saved-key\n")
        .assert()
        .success();
    command(home.path())?
        .args([
            "setup",
            "--base-url",
            "http://localhost:8080/v1/",
            "--model",
            "new-model",
        ])
        .assert()
        .success();
    let config = std::fs::read_to_string(home.path().join("config.toml"))?;
    assert!(config.contains("# keep this comment"));
    let parsed: toml::Value = toml::from_str(&config)?;
    assert_eq!(parsed["show_raw_agent_reasoning"].as_bool(), Some(false));
    assert_eq!(parsed["model"].as_str(), Some("new-model"));
    assert_eq!(
        parsed["model_providers"]["rcp"]["base_url"].as_str(),
        Some("http://localhost:8080/v1")
    );
    let auth: Value =
        serde_json::from_str(&std::fs::read_to_string(home.path().join("auth.json"))?)?;
    assert_eq!(auth["LEMEX_API_KEY"], "saved-key");
    Ok(())
}

#[test]
fn invalid_inputs_do_not_create_config_or_credentials() -> Result<()> {
    for (args, key) in [
        (vec!["setup", "--with-api-key"], "\n"),
        (vec!["setup", "--with-api-key"], "one\ntwo\n"),
        (
            vec!["setup", "--with-api-key", "--base-url", "file:///tmp/key"],
            "key",
        ),
        (vec!["setup", "--with-api-key", "--model", " "], "key"),
        (vec!["setup"], ""),
    ] {
        let home = TempDir::new()?;
        command(home.path())?
            .args(args)
            .write_stdin(key)
            .assert()
            .failure();
        assert!(!home.path().join("config.toml").exists());
        assert!(!home.path().join("auth.json").exists());
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_commands_use_live_ids_and_validate_each_models_reasoning() -> Result<()> {
    let server = MockServer::start().await;
    let home = TempDir::new()?;
    command(home.path())?
        .args([
            "setup",
            "--with-api-key",
            "--base-url",
            &format!("{}/v1", server.uri()),
        ])
        .write_stdin("model-selection-secret\n")
        .assert()
        .success();
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer model-selection-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[
            {"id":"deepseek-ai/DeepSeek-V4.1-Flash-1017"}, {"id":"Qwen/Qwen3.5-397B-A17B"},
            {"id":"Qwen/Qwen3-Embedding-8B"}
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/model/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[
            {"model_name":"Qwen/Qwen3-Embedding-8B","model_info":{"mode":"embedding"}}
        ]})))
        .mount(&server)
        .await;
    let output = command(home.path())?
        .args(["models", "--all", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("model-selection-secret"));
    let listing: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(listing["source"], "live");
    assert_eq!(listing["models"].as_array().unwrap().len(), 2);
    let saved_listing = command(home.path())?
        .args(["models", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let saved_listing: Value = serde_json::from_slice(&saved_listing)?;
    assert_eq!(saved_listing["models"].as_array().unwrap().len(), 6);
    assert!(
        saved_listing["models"]
            .as_array()
            .unwrap()
            .iter()
            .all(|model| model["added"] == true)
    );
    assert!(
        saved_listing["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["available"] == false)
    );
    assert!(
        !saved_listing["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["id"] == "Qwen/Qwen3.5-397B-A17B")
    );
    let search = command(home.path())?
        .args(["models", "--search", "QWEN3.5", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let search: Value = serde_json::from_slice(&search)?;
    assert_eq!(search["models"].as_array().unwrap().len(), 1);
    assert_eq!(search["models"][0]["added"], false);
    let before_add: toml::Value =
        toml::from_str(&std::fs::read_to_string(home.path().join("config.toml"))?)?;
    for _ in 0..2 {
        command(home.path())?
            .args(["models", "add", "Qwen/Qwen3.5-397B-A17B"])
            .assert()
            .success();
    }
    let after_add: toml::Value =
        toml::from_str(&std::fs::read_to_string(home.path().join("config.toml"))?)?;
    assert_eq!(after_add["model"], before_add["model"]);
    assert_eq!(
        after_add["model_picker_models"].as_array().unwrap().len(),
        7
    );
    command(home.path())?
        .args(["models", "remove", "Qwen/Qwen3.5-397B-A17B"])
        .assert()
        .success();
    command(home.path())?
        .args(["models", "reset"])
        .assert()
        .success();
    let deepseek = listing["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["id"] == "deepseek-ai/DeepSeek-V4.1-Flash-1017")
        .unwrap();
    assert_eq!(deepseek["default_reasoning_effort"], "max");
    assert_eq!(
        deepseek["reasoning_efforts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["effort"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["none", "low", "high", "xhigh", "max"]
    );
    let before = std::fs::read(home.path().join("config.toml"))?;
    command(home.path())?
        .args([
            "model",
            "deepseek-ai/DeepSeek-V4.1-Flash-1017",
            "--reasoning",
            "medium",
        ])
        .assert()
        .failure()
        .stderr(contains("is not supported"));
    assert_eq!(std::fs::read(home.path().join("config.toml"))?, before);
    command(home.path())?
        .args(["model", "removed/model", "--reasoning", "high"])
        .assert()
        .failure()
        .stderr(contains("not listed"));
    command(home.path())?
        .args([
            "model",
            "deepseek-ai/DeepSeek-V4.1-Flash-1017",
            "--reasoning",
            "max",
        ])
        .assert()
        .success();
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(home.path().join("config.toml"))?)?;
    assert_eq!(config["model_reasoning_effort"].as_str(), Some("max"));
    command(home.path())?
        .args(["model", "Qwen/Qwen3.5-397B-A17B"])
        .assert()
        .success();
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(home.path().join("config.toml"))?)?;
    assert_eq!(config["model"].as_str(), Some("Qwen/Qwen3.5-397B-A17B"));
    assert!(config.get("model_reasoning_effort").is_none());
    assert_eq!(
        config["model_picker_models"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_str(),
        Some("Qwen/Qwen3.5-397B-A17B")
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saved_key_reaches_responses_api_and_environment_overrides_work() -> Result<()> {
    for override_environment in [false, true] {
        let server = MockServer::start().await;
        let home = TempDir::new()?;
        let key = if override_environment {
            "override-key"
        } else {
            "saved-key"
        };
        let event = json!({"type":"response.completed", "response": {
            "id":"setup-test-response", "status":"completed", "output":[],
            "usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}
        }});
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .and(header("authorization", format!("Bearer {key}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(format!("event: response.completed\ndata: {event}\n\n")),
            )
            .expect(1)
            .mount(&server)
            .await;
        let endpoint = format!("{}/v1", server.uri());
        let saved_endpoint = if override_environment {
            "http://127.0.0.1:1/v1"
        } else {
            &endpoint
        };
        command(home.path())?
            .args(["setup", "--with-api-key", "--base-url", saved_endpoint])
            .write_stdin("saved-key\n")
            .assert()
            .success();
        let mut exec = command(home.path())?;
        if override_environment {
            exec.env("LEMEX_API_KEY", key)
                .env("LEMEX_BASE_URL", &endpoint);
        }
        exec.args([
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "-c",
            "mcp_servers.exa.enabled=false",
            "Reply exactly OK",
        ])
        .assert()
        .success();
        server.verify().await;
    }
    Ok(())
}

#[test]
fn doctor_recognizes_saved_key_without_environment_exports() -> Result<()> {
    let home = TempDir::new()?;
    command(home.path())?
        .args([
            "setup",
            "--with-api-key",
            "--base-url",
            "http://127.0.0.1:1/v1",
        ])
        .write_stdin("doctor-test-key\n")
        .assert()
        .success();
    let output = command(home.path())?
        .args(["doctor", "--json"])
        .assert()
        .get_output()
        .clone();
    let report: Value = serde_json::from_slice(&output.stdout)?;
    let auth = &report["checks"]["auth.credentials"];
    assert_eq!(auth["status"], "ok");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("doctor-test-key"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_rejects_browser_device_and_account_token_login() -> Result<()> {
    use codex_app_server_protocol::RequestId;
    let home = TempDir::new()?;
    let binary = codex_utils_cargo_bin::cargo_bin("lemex")?;
    let mut server = app_test_support::TestAppServer::builder()
        .with_program(&binary)
        .with_codex_home(home.path())
        .without_auto_env()
        .without_managed_config()
        .with_plugin_startup_tasks()
        .with_env_overrides(&[
            ("LEMEX_API_KEY", None),
            ("LEMEX_ACCESS_TOKEN", None),
            ("OPENAI_FEDERATION_RULE_ID", None),
            ("OPENAI_IDENTITY_TOKEN_FILE", None),
        ])
        .with_args(&["app-server"])
        .build_initialized()
        .await?;
    for params in [
        json!({"type":"chatgpt"}),
        json!({"type":"chatgptDeviceCode"}),
        json!({"type":"chatgptAuthTokens", "accessToken":"account-token", "chatgptAccountId":"account"}),
    ] {
        let id = server.send_login_account_request(params).await?;
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            server.read_stream_until_error_message(RequestId::Integer(id)),
        )
        .await??;
        assert!(error.error.message.contains("Lemex uses API keys"));
        assert!(!home.path().join("auth.json").exists());
    }
    server.shutdown_gracefully().await?;
    Ok(())
}
