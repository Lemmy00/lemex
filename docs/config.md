# Configuration

Lemex reads `~/.lemex/config.toml` and stores its state in `~/.lemex`. Set `LEMEX_HOME` before installation and launching Lemex to use another directory. The source installer copies the bundled [configuration](../config/config.toml) and [model catalog](../config/models.json) when those files are missing.

## Environment variables

Run `lemex setup` to save an API key and endpoint. The default endpoint is RCP (`https://inference.rcp.epfl.ch/v1`). No environment variables are required after setup.

| Variable         | When to set it |
| ---------------- | -------------- |
| `LEMEX_API_KEY`  | Optional API key; overrides the saved key for the built-in RCP provider. |
| `LEMEX_BASE_URL` | Optional endpoint override; takes precedence over the saved RCP URL. |
| `LEMEX_HOME`     | Optional configuration and state directory; defaults to `~/.lemex`. |

Custom providers read the environment variable named by their `env_key` setting.

## Settings

The bundled configuration selects the RCP provider, loads `models.json`, uses a 240,000-token context window with compaction at 220,000 tokens, shows raw reasoning, and disables automatic daemon startup. It includes the requested trusted project paths and marks screen-reader detection complete. Edit `config.toml` to adapt these defaults:

| Setting                          | Purpose                                                                     |
| -------------------------------- | --------------------------------------------------------------------------- |
| `model`                          | Default model ID. Choose one available from your provider and catalog.      |
| `model_provider`                 | Provider selected from `model_providers` or the built-in providers.         |
| `model_catalog_json`             | Model catalog path, relative to the configuration file or absolute.         |
| `model_picker_models`            | Saved model IDs in `/model`; RCP defaults to the six bundled curated models. |
| `model_context_window`           | Context window to use, within the model's supported limit.                  |
| `model_auto_compact_token_limit` | Token threshold for automatic context compaction.                           |
| `model_reasoning_effort`         | Optional reasoning effort; leave unset to use each model's catalog default. |
| `show_raw_agent_reasoning`       | Show provider reasoning; defaults to `true` even when omitted.              |
| `mcp_servers`                    | MCP servers that provide additional tools.                                  |

Keep top-level settings above any `[table]` headers. Restart Lemex after editing configuration files. Use `/model` to select a model interactively or override settings for one launch:

```sh
lemex -m "<model-id>"
lemex -c show_raw_agent_reasoning=false
```

The default is `deepseek-ai/DeepSeek-V4.1-Flash-1017` with `max` reasoning.
When overriding the model on the command line, also select an effort that the
new model supports, since `-m` alone retains the configured reasoning effort:

```sh
lemex -m "mistralai/Leanstral-2603" -c model_reasoning_effort="high"
lemex -m "Qwen/Qwen3.8-Flash-Next" -c model_reasoning_effort="xhigh"
```

## Change provider

To point the built-in RCP provider at another Responses API endpoint:

```sh
export LEMEX_BASE_URL="https://api.example.com/v1"
```

It uses the saved API key or `LEMEX_API_KEY`. To save a URL and model instead:

```sh
lemex setup --base-url "https://api.example.com/v1" --model "your-model-id"
```

For another provider that supports the Responses API, replace the provider selection and add a provider table:

```toml
# Top-level settings, before any tables.
model_provider = "custom"
model = "your-model-id"
model_picker_models = [] # Start with an empty shortlist.

[model_providers.custom]
name = "My provider"
base_url = "https://api.example.com/v1"
env_key = "MY_PROVIDER_API_KEY"
wire_api = "responses"
requires_openai_auth = false
```

Set `MY_PROVIDER_API_KEY` in your environment, then run `lemex models --all`.
Lemex discovers models from the selected provider's authenticated `/models`
endpoint. Both standard `data` lists and rich Lemex catalogs are accepted;
a local `models.json` is optional. Existing local metadata is an offline fallback,
and a successful live list determines which models are currently listed.

Discovery excludes deployments explicitly advertised as incompatible with text,
Responses, or tool calling. An ID-only listing remains a candidate: it does not
verify inference access, deployment health, or tool calling. A provider offering
only Chat Completions still needs a gateway that implements Responses for inference.

## Model catalog and tools

For an online provider, use discovery and shortlist controls without editing a
catalog. An optional local `models.json` supplies offline metadata; its entries
can specify names, context limits, reasoning levels, and capabilities. Lemex
limits `model_context_window` to an advertised `max_context_window`.

Use `lemex models` for the saved list and current availability. `lemex models --all` browses everything available to your key, and `lemex models --search QUERY` searches names and IDs. Add an entry with `lemex models add MODEL_ID`, or choose **Add model…** in `/model` to search and add interactively. The default saved list contains only the six curated models below; `lemex models reset` restores it. The bundled catalog supplies verified reasoning profiles and an offline fallback.

`lemex models shortlist` edits the shortlist interactively. Pass model IDs to
replace it in one step (`lemex models shortlist MODEL_A MODEL_B`), or use
`lemex models shortlist --clear` to empty it. Shortlist edits do not change the
current session, default model, or reasoning preference. In `/model`, **Add
model…** opens the same independent add/remove flow; return to **Your models**
to select a default and its reasoning. Custom providers initially shortlist only
the configured default, rather than every discovered model.

`lemex model MODEL --reasoning EFFORT` validates the model and its effort before saving and adds it to the list. Omitting `--reasoning` clears the previous model's override. `lemex model` opens the saved-list and search flow in the terminal.

Live, explicitly advertised reasoning values take priority. The verified bundled
profiles below are used for the RCP provider only. Other providers do not inherit
RCP effort values just because a model ID matches. A generic reasoning capability
flag does not establish which effort strings work: unknown settings use the
deployment default and omit an explicit effort. Lemex also omits an unadvertised
effort left in configuration when sending a request to a third-party provider.

The bundled RCP catalog was checked on 2026-10-18:

| Model ID | Reasoning efforts | Catalog default |
| --- | --- | --- |
| `deepseek-ai/DeepSeek-V4.1-Flash-1017` | `none`, `low`, `high`, `xhigh`, `max` | `max` |
| `deepseek-ai/DeepSeek-V4.1-Flash` | `none`, `low`, `high`, `xhigh`, `max` | `high` |
| `Qwen/Qwen3.8-Flash-Next` | `none`, `low`, `medium`, `xhigh` | `medium` |
| `Qwen/Qwen3.8-27B` | `none`, `low`, `medium`, `xhigh` | `medium` |
| `zai-org/GLM-5.3-Flash` | `low`, `high`, `max` | `max` |
| `mistralai/Leanstral-2603` | `none`, `high` | `high` |

`none` disables reasoning where supported. DeepSeek V4.1 rejects `minimal` and
`medium`; Qwen 3.8 rejects `minimal`, `high`, and `max`.
[Qwen's template](https://huggingface.co/Qwen/Qwen3.8-27B/blob/main/chat_template.jinja)
defines its three reasoning levels.
[GLM's model card](https://huggingface.co/zai-org/GLM-5.3-Flash/blob/main/README.md)
specifies `low`, `high`, and `max`: other strings silently select `max`, so HTTP
acceptance alone does not establish a distinct reasoning level.
[Leanstral's template](https://huggingface.co/mistralai/Leanstral-2603/blob/main/chat_template.jinja)
permits only `none` and `high`.

All listed DeepSeek, Qwen, and GLM effort values were accepted by live RCP
Responses API probes. Leanstral's values are verified against its official
template; RCP listed the model but inference still timed out after a five-minute
attempt, so live Leanstral availability was not confirmed. Qwen 3.8 27B also
initially timed out, then responded after about three minutes and passed the
effort checks. The configured DeepSeek 1017 / `max` default passed an ephemeral
Lemex CLI request.

The bundled RCP provider disables standalone hosted web search. Add MCP servers for web search or other tools with `lemex mcp` and inspect connected tools with `/mcp`. Enable hosted search only if your provider supports it.

Lemex shows raw reasoning returned by the provider by default, including for existing configuration files that omit `show_raw_agent_reasoning`. Set it to `false` to hide raw reasoning.

Press **Ctrl+T** to view completed reasoning blocks in the expanded transcript. The compact chat shows the current reasoning activity in its status line.

## Managed hooks

Admins can set `allow_managed_hooks_only = true` in `requirements.toml` to ignore user, project, and session hooks while allowing managed hooks. This setting belongs in `requirements.toml`.

## Background daemon

Normal terminal sessions, resume/fork, and `lemex exec` use the embedded backend.
They do not start a persistent daemon or attach to one left by an older install.

For shared sessions and work that continues after closing the terminal, opt in:

```sh
lemex --enable daemon_auto_start
```

To save that preference, run `lemex features enable daemon_auto_start`, or add:

```toml
[features]
daemon_auto_start = true
```

`lemex --no-daemon` overrides the preference for one launch. `lemex agents` and
explicit daemon or remote-control commands request a persistent backend directly.

The daemon never downloads releases or updates itself. After updating Lemex,
refresh an explicitly installed daemon with `lemex app-server daemon update`.
Stop it with `lemex app-server daemon stop`. These commands may interrupt running
work; ordinary CLI upgrades do not replace or stop an active daemon.
