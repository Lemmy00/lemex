//! Prevent inherited paths from reviving the removed plugin-reporting sidecar.
use std::collections::HashMap;
pub const PLUGIN_METRICS_OUTPUT_ENV_VAR: &str = "CODEX_PLUGIN_METRICS_OUTPUT";

pub fn strip_output_env(env: &mut HashMap<String, String>) {
    if cfg!(windows) {
        env.retain(|key, _| !key.eq_ignore_ascii_case(PLUGIN_METRICS_OUTPUT_ENV_VAR));
    } else {
        env.remove(PLUGIN_METRICS_OUTPUT_ENV_VAR);
    }
}
