# Configuration

Lemex reads `~/.lemex/config.toml` and stores its state in `~/.lemex`. Set `LEMEX_HOME` before installation and launching Lemex to use another directory. The source installer copies the bundled [configuration](../config/config.toml) and [model catalog](../config/models.json) when those files are missing.

## Environment variables

For the bundled configuration, set the API key for the RCP endpoint:

```sh
export LEMEX_API_KEY="your-api-key"
```

Save the export in your shell profile (`~/.zshrc` or `~/.bashrc`) and reload it, or open a new terminal.

| Variable             | When to set it                                                                                                                                           |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LEMEX_API_KEY`      | Required by the bundled provider configuration.                                                                                                          |
| `LEMEX_BASE_URL`     | Optional override for the built-in RCP endpoint, which defaults to `https://inference.rcp.epfl.ch/v1`. Custom providers use their configured `base_url`. |
| `LEMEX_HOME`         | Optional configuration and state directory; defaults to `~/.lemex`.                                                                                      |
| `LEMEX_ACCESS_TOKEN` | Only for an access-token authentication setup; unused by the bundled provider.                                                                           |

Custom providers read the environment variable named by their `env_key` setting.

## Settings

The bundled configuration selects the RCP provider, loads `models.json`, uses a 200,000-token context window with compaction at 180,000 tokens, shows raw reasoning, and adds Exa MCP tools for web search. Edit `config.toml` to adapt these defaults:

| Setting                          | Purpose                                                                     |
| -------------------------------- | --------------------------------------------------------------------------- |
| `model`                          | Default model ID. Choose one available from your provider and catalog.      |
| `model_provider`                 | Provider selected from `model_providers` or the built-in providers.         |
| `model_catalog_json`             | Model catalog path, relative to the configuration file or absolute.         |
| `model_context_window`           | Context window to use, within the model's supported limit.                  |
| `model_auto_compact_token_limit` | Token threshold for automatic context compaction.                           |
| `model_reasoning_effort`         | Optional reasoning effort; leave unset to use each model's catalog default. |
| `show_raw_agent_reasoning`       | Show or hide reasoning text returned by the provider.                       |
| `mcp_servers`                    | MCP servers that provide additional tools.                                  |

Keep top-level settings above any `[table]` headers. Restart Lemex after editing configuration files. Use `/model` to select a model interactively or override settings for one launch:

```sh
lemex -m "<model-id>"
lemex -c show_raw_agent_reasoning=false
```

## Change provider

To point the built-in RCP provider at another Responses API endpoint:

```sh
export LEMEX_BASE_URL="https://api.example.com/v1"
```

It continues to use `LEMEX_API_KEY`.

For another provider that supports the Responses API, replace the provider selection and add a provider table:

```toml
# Top-level settings, before any tables.
model_provider = "custom"
model = "your-model-id"
model_catalog_json = "models.json"

[model_providers.custom]
name = "My provider"
base_url = "https://api.example.com/v1"
env_key = "MY_PROVIDER_API_KEY"
wire_api = "responses"
requires_openai_auth = false
```

Set `MY_PROVIDER_API_KEY` in your environment and adapt `models.json` to match the new provider.

## Model catalog and tools

To add a model, copy a suitable entry in `models.json` and update its `slug`, display name, reasoning levels, context limits, and capabilities to match the deployment. The default `model` must match a catalog entry. Lemex limits `model_context_window` to the catalog's `max_context_window`.

The bundled config uses MCP tools for web search and sets `web_search = "disabled"` for the provider's hosted search. Manage servers with `lemex mcp` and inspect connected tools with `/mcp`. Enable hosted search only if your provider supports it.

Press **Ctrl+T** to view the expanded transcript, including reasoning returned by the provider.

## Managed hooks

Admins can set `allow_managed_hooks_only = true` in `requirements.toml` to ignore user, project, and session hooks while allowing managed hooks. This setting belongs in `requirements.toml`.
