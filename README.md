# Lemex CLI

Lemex CLI is a fork of the OpenAI Codex coding agent that runs locally on your computer. It is intentionally separated from the official `codex` CLI so you can run both side-by-side without sharing configuration, API keys, or login state.

## Quickstart

### Installing and running Lemex CLI

#### Option 1: Install from npm (once published)

```shell
npm install -g lemex
```

> **Note:** `lemex` is not yet published to npm. Until the first release is published, install from source using Option 2 below.

#### Option 2: Install from source using the convenience script

```shell
git clone https://github.com/Lemmy00/lemex.git
cd lemex
./scripts/install/install_from_source.sh
```

The script checks for Node.js/npm, installs the Rust toolchain if necessary, builds the CLI, installs the `lemex` command globally via npm, and copies a default RCP config (`config/config.toml` and `config/models.json`) into `~/.lemex` if none exists.

To also copy an existing Lemex config from another machine (overriding the default):

```bash
LEMEX_COPY_CONFIG_FROM=otherhost:/home/you/.lemex ./scripts/install/install_from_source.sh
```

On Ubuntu, source builds need Python 3, `ripgrep`, `build-essential`, `pkg-config`, and
`libssl-dev`, and on Linux `libcap-dev`, in addition to Node.js/npm and Rust. The installer downloads
checksum-verified V8 binaries instead of compiling V8 from source.

Then run `lemex` from anywhere.

### Configuration

Lemex reads its own environment variables and config directory, so it will not interfere with an existing Codex installation:

- `LEMEX_API_KEY` — API key for the model provider.
- `LEMEX_BASE_URL` — Base URL for the model provider (defaults to `https://inference.rcp.epfl.ch/v1`).
- `LEMEX_ACCESS_TOKEN` — ChatGPT-plan access token, if used.
- `LEMEX_HOME` — Lemex config/state directory (defaults to `~/.lemex`).

Add the exports to your shell profile (e.g. `~/.zshrc`) so they persist:

```shell
export LEMEX_API_KEY="sk-..."
export LEMEX_BASE_URL="https://inference.rcp.epfl.ch/v1"
```

Then reload your profile:

```shell
source ~/.zshrc
```

When `LEMEX_API_KEY` is set and no `model_provider` is configured, Lemex defaults to the built-in `rcp` provider. Otherwise it falls back to the `openai` provider.

### RCP model catalog

To use models available from the RCP inference endpoint, put a `models.json` catalog in `~/.lemex` and reference it from `~/.lemex/config.toml`:

```toml
model_provider = "rcp"
model = "moonshotai/Kimi-K2.7-Code"
model_catalog_json = "models.json"

[model_providers.rcp]
name = "RCP"
base_url = "https://inference.rcp.epfl.ch/v1"
env_key = "LEMEX_API_KEY"
wire_api = "responses"
requires_openai_auth = false
```

Use `lemex exec -m <model-id> ...` to select a different model from the catalog for a single run.

For DeepSeek V4.1 Flash on RCP:

```shell
lemex -m deepseek-ai/DeepSeek-V4.1-Flash
```

The catalog also includes Qwen 3.8 Flash Next and Qwen 3.8 27B:

```shell
lemex -m Qwen/Qwen3.8-Flash-Next -c 'model_reasoning_effort="xhigh"'
lemex -m Qwen/Qwen3.8-27B -c 'model_reasoning_effort="xhigh"'
```

These Qwen models accept `low`, `medium`, and `xhigh` reasoning on RCP;
`high` is rejected. The explicit override also works when your saved default
reasoning effort is `high` for a different model.

The source installer builds only Lemex and its required helpers, strips release
symbols, and removes its temporary build directory on exit. Set `LEMEX_KEEP_BUILD=1`
to retain that directory, or set `CARGO_TARGET_DIR` to reuse a build cache that the
installer will leave in place.
The runtime package also includes ripgrep, package metadata, and compatibility
names required by the upstream background server, so plain `lemex` can start it.

### Health check

```shell
lemex doctor
lemex --version
```

## Docs

- [**Installing & building**](./docs/install.md)
- [**Contributing**](./docs/contributing.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).
