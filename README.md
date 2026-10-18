# Lemex CLI

Lemex is a local coding agent based on OpenAI Codex. It runs as `lemex` and keeps its configuration and state in `~/.lemex`, so you can use it alongside Codex.

External analytics and feedback reporting are removed. Local logs and token usage are preserved. Custom OpenTelemetry exporters require explicit configuration.

Lemex does not include ChatGPT cloud tasks, account sign-in, or the Codex desktop-app installer. Developer test tooling is kept outside the installed CLI.

Normal sessions use the embedded backend. The persistent background daemon is opt-in, and never downloads or updates itself.

## Install

On macOS, Linux, or Windows through WSL2, install Git, curl, Node.js/npm, Python 3.11+, and a C/C++ toolchain (Xcode Command Line Tools on macOS). The installer prepares the [remaining build dependencies](docs/install.md#requirements), including a user-cache fallback on Linux without sudo. Run:

```sh
git clone --depth 1 https://github.com/Lemmy00/lemex.git
cd lemex
./scripts/install/install_from_source.sh
```

The installer builds and installs `lemex` globally, installs Rust if needed, and copies the bundled configuration and model catalog into `~/.lemex`. Existing configuration files are preserved. Temporary build and packaging caches are removed automatically, including after a failed or interrupted install.

Fresh installations default to RCP's `deepseek-ai/DeepSeek-V4.1-Flash-1017` with `max` reasoning. The bundled catalog includes DeepSeek V4.1, Qwen 3.8, GLM 5.3 Flash, and Leanstral; see [available models and reasoning efforts](docs/config.md#model-catalog-and-tools).

## Run

Set up your API key once, then start Lemex:

```sh
lemex setup
lemex
```

Setup prompts for a hidden API key and an API URL, defaulting to `https://inference.rcp.epfl.ch/v1`. It saves the key in your configured credential store, separately from `config.toml`. Lemex uses API keys; there is no OpenAI or ChatGPT login.

For scripts, you can instead set `LEMEX_API_KEY` in the environment. To change the saved URL or model, run `lemex setup --base-url "https://api.example.com/v1" --model "your-model-id"`.

For a single task, run from your project's Git repository:

```sh
lemex exec "Explain this codebase"
```

Outside a Git repository, add `--skip-git-repo-check`.

## Customize

Edit `~/.lemex/config.toml` to change the default model, provider, context limits, reasoning display, or MCP servers. Lemex refreshes RCP's model list; the installed `models.json` supplies metadata and an offline fallback. Use `/model` in Lemex to choose a model, or `lemex -m "<model-id>"` for one launch.

The default `/model` picker shows the six curated models and their availability. Choose **Add model…** to search models available to your key and add one to the saved list. In the terminal, use `lemex models --search QUERY`, `lemex models add MODEL_ID`, and `lemex model`. See [model selection](docs/authentication.md#model-selection) for reasoning settings and list management.

Optional environment variables:

| Variable         | Purpose                                                                                                             |
| ---------------- | ------------------------------------------------------------------------------------------------------------------- |
| `LEMEX_HOME`     | Configuration and state directory; defaults to `~/.lemex`. Set it before installation to use a different directory. |
| `LEMEX_BASE_URL` | Override the default RCP endpoint (`https://inference.rcp.epfl.ch/v1`).                                             |

See [Configuration](docs/config.md) for provider setup and catalog customization, and [Installation](docs/install.md) for build options and config import.

To check your setup, run `lemex doctor`.

Licensed under [Apache-2.0](LICENSE).
