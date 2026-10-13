//! Default embedded launches ignore a daemon even when other features are overridden.

use super::focus_palette::PtyCodex;
use super::focus_palette::write_test_config;
use anyhow::Result;
use anyhow::ensure;

#[test]
fn default_and_feature_overridden_launches_do_not_discover_existing_daemons() -> Result<()> {
    for scenario in ["default", "explicit", "host policy"] {
        let cwd = codex_utils_cargo_bin::repo_root()?;
        let home = tempfile::tempdir_in("/tmp")?;
        write_test_config(home.path(), &cwd)?;
        if scenario == "host policy" {
            let path = home.path().join("config.toml");
            let contents = std::fs::read_to_string(&path)?;
            std::fs::write(
                path,
                format!(
                    "features.code_mode_host = {{enabled=false, disable_in_process_fallback=true}}\n{contents}"
                ),
            )?;
        }
        let socket = codex_app_server_client::app_server_control_socket_path(home.path())?;
        std::fs::create_dir_all(socket.parent().unwrap())?;
        let listener = std::os::unix::net::UnixListener::bind(socket.as_path())?;
        listener.set_nonblocking(true)?;
        let args = if scenario == "explicit" {
            vec!["-c", "features.api_key_model_discovery=false"]
        } else {
            vec![]
        };
        let mut terminal = PtyCodex::start(&cwd, home, &args)?;
        terminal.wait_for_startup()?;
        terminal.write_input(b"/status")?;
        terminal.wait_for_screen("show current session configuration")?;
        terminal.read_output(std::time::Duration::from_millis(200))?;
        terminal.write_input(b"\r")?;
        terminal.wait_for_screen("Model:")?;
        ensure!(!terminal.screen_contains("unix://"));
        ensure!(
            listener
                .accept()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::WouldBlock),
            "an embedded launch attempted to discover the existing daemon"
        );
    }
    Ok(())
}
