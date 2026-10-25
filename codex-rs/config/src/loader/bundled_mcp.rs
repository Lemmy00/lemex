use crate::ConfigLayerSource;
use crate::McpServerConfig;
use crate::state::ConfigLayerEntry;
use toml::Value;

/// The npm launcher discovers its own optional dependency. Keep that default
/// below every configured layer, without persisting installation-specific paths.
pub(super) fn add_ultracode_defaults(layers: &mut [ConfigLayerEntry], config: Option<&str>) {
    let Some(config) = config else {
        return;
    };
    // A complete user registration owns its transport, including remote URLs.
    // Settings-only overrides (enabled, timeouts, tools) augment the default.
    if layers.iter().any(|layer| {
        layer.disabled_reason.is_none()
            && layer
                .config
                .get("mcp_servers")
                .and_then(|servers| servers.get("ultracode"))
                .is_some_and(|server| {
                    server.get("command").is_some() || server.get("url").is_some()
                })
    }) {
        return;
    }
    let server = match serde_json::from_str::<McpServerConfig>(config)
        .and_then(|server| Value::try_from(server).map_err(serde::ser::Error::custom))
    {
        Ok(server) => server,
        Err(error) => {
            tracing::warn!(%error, "Ignoring invalid bundled UltraCode configuration");
            return;
        }
    };
    let Some(layer) = layers
        .iter_mut()
        .find(|layer| matches!(layer.name, ConfigLayerSource::PackagedDefaults { .. }))
    else {
        return;
    };
    let Some(config) = layer.config.as_table_mut() else {
        return;
    };
    let Some(servers) = config
        .entry("mcp_servers")
        .or_insert_with(|| Value::Table(Default::default()))
        .as_table_mut()
    else {
        return;
    };
    servers.entry("ultracode").or_insert(server);
    *layer = ConfigLayerEntry::new(layer.name.clone(), layer.config.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_toml::ConfigToml;
    use crate::merge_toml_values;
    use codex_utils_absolute_path::AbsolutePathBuf;

    const SERVER: &str = r#"{"command":"/node","args":["/package with spaces/ultracode.js","--stdio"],"startup_timeout_sec":60}"#;

    fn layers(user: &str) -> Vec<ConfigLayerEntry> {
        let file = AbsolutePathBuf::resolve_path_against_base("config.toml", std::env::temp_dir());
        vec![
            ConfigLayerEntry::new(
                ConfigLayerSource::PackagedDefaults { file: file.clone() },
                Value::Table(Default::default()),
            ),
            ConfigLayerEntry::new(
                ConfigLayerSource::User {
                    file,
                    profile: None,
                },
                toml::from_str(user).unwrap(),
            ),
        ]
    }

    fn effective(layers: &[ConfigLayerEntry]) -> ConfigToml {
        let mut config = Value::Table(Default::default());
        for layer in layers
            .iter()
            .filter(|layer| layer.disabled_reason.is_none())
        {
            merge_toml_values(&mut config, &layer.config);
        }
        config.try_into().unwrap()
    }

    #[test]
    fn bundled_server_is_available_without_writing_user_config() {
        let mut layers = layers("");
        let original = layers[1].clone();
        add_ultracode_defaults(&mut layers, Some(SERVER));
        let config = effective(&layers);
        assert!(config.mcp_servers["ultracode"].enabled);
        assert!(!config.mcp_servers["ultracode"].required);
        assert_eq!(layers[1], original);
    }

    #[test]
    fn settings_can_disable_or_customize_the_bundled_server() {
        let mut layers = layers("[mcp_servers.ultracode]\nenabled = false\ntool_timeout_sec = 42");
        add_ultracode_defaults(&mut layers, Some(SERVER));
        let config = effective(&layers);
        let server = &config.mcp_servers["ultracode"];
        assert!(!server.enabled);
        assert_eq!(
            server.tool_timeout_sec,
            Some(std::time::Duration::from_secs(42))
        );
    }

    #[test]
    fn explicit_stdio_and_http_registrations_are_preserved() {
        for transport in [
            "command = 'custom-ultracode'",
            "url = 'https://example.com/mcp'",
        ] {
            let mut layers = layers(&format!("[mcp_servers.ultracode]\n{transport}"));
            let original = effective(&layers);
            add_ultracode_defaults(&mut layers, Some(SERVER));
            assert_eq!(effective(&layers).mcp_servers, original.mcp_servers);
        }
    }

    #[test]
    fn absent_or_invalid_optional_dependency_does_not_add_a_server() {
        for config in [None, Some("invalid json"), Some(r#"{"enabled":true}"#)] {
            let mut layers = layers("");
            add_ultracode_defaults(&mut layers, config);
            assert!(effective(&layers).mcp_servers.is_empty());
        }
    }
}
