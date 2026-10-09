# Lemex CLI

Lemex is a local coding agent based on OpenAI Codex. It runs as `lemex` and keeps its configuration and login state in `~/.lemex`, so you can use it alongside Codex.

## Install

On macOS, Linux, or Windows through WSL2, install Git, Node.js/npm, Python 3.11+, ripgrep, and the [platform build dependencies](docs/install.md#requirements). Then run:

```sh
git clone https://github.com/Lemmy00/lemex.git
cd lemex
./scripts/install/install_from_source.sh
```

The installer builds and installs `lemex` globally, installs Rust if needed, and copies the bundled configuration and model catalog into `~/.lemex`. Existing configuration files are preserved.

## Run

The default configuration uses the RCP inference endpoint. Set its API key:

```sh
export LEMEX_API_KEY="your-api-key"
lemex
```

Add the export to your shell profile (`~/.zshrc` or `~/.bashrc`) to keep it across sessions, then reload the profile or open a new terminal. This is the only environment variable required by the default configuration.

For a single task:

```sh
lemex exec "Explain this codebase"
```

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
