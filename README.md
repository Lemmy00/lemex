# Lemex CLI

Lemex is a local coding agent based on OpenAI Codex. It runs as `lemex` and keeps its configuration and state in `~/.lemex`, so you can use it alongside Codex.

External analytics and feedback reporting are removed. Local logs and token usage are preserved. Custom OpenTelemetry exporters require explicit configuration.

Normal sessions use the embedded backend. The persistent background daemon is opt-in, and never downloads or updates itself.

## Install

On macOS, Linux, or Windows through WSL2, install Git, Node.js/npm, Python 3.11+, ripgrep, and the [platform build dependencies](docs/install.md#requirements). Then run:

```sh
git clone https://github.com/Lemmy00/lemex.git
cd lemex
./scripts/install/install_from_source.sh
```

The installer builds and installs `lemex` globally, installs Rust if needed, and copies the bundled configuration and model catalog into `~/.lemex`. Existing configuration files are preserved. Temporary build and packaging caches are removed automatically, including after a failed or interrupted install.

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

Edit `~/.lemex/config.toml` to change the default model, provider, context limits, reasoning display, or MCP servers. The installed `models.json` contains the model catalog; use `/model` in Lemex to choose a model, or `lemex -m "<model-id>"` for one launch.

Optional environment variables:

| Variable         | Purpose                                                                                                             |
| ---------------- | ------------------------------------------------------------------------------------------------------------------- |
| `LEMEX_HOME`     | Configuration and state directory; defaults to `~/.lemex`. Set it before installation to use a different directory. |
| `LEMEX_BASE_URL` | Override the default RCP endpoint (`https://inference.rcp.epfl.ch/v1`).                                             |

See [Configuration](docs/config.md) for provider setup and catalog customization, and [Installation](docs/install.md) for build options and config import.

To check your setup, run `lemex doctor`.

Licensed under [Apache-2.0](LICENSE).
