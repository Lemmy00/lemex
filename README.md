# Lemex CLI

Lemex CLI is a fork of the OpenAI Codex coding agent that runs locally on your computer. It is intentionally separated from the official `codex` CLI so you can run both side-by-side without sharing configuration, API keys, or login state.

## Quickstart

### Installing and running Lemex CLI

Install from npm:

```shell
npm install -g lemex
```

Then run:

```shell
lemex
```

### Configuration

Lemex reads its own environment variables and config directory, so it will not interfere with an existing Codex installation:

- `LEMEX_API_KEY` — API key for the model provider.
- `LEMEX_BASE_URL` — Base URL for the model provider (defaults to `https://inference.rcp.epfl.ch/v1`).
- `LEMEX_ACCESS_TOKEN` — ChatGPT-plan access token, if used.
- `LEMEX_HOME` — Lemex config/state directory (defaults to `~/.lemex`).

Example:

```shell
export LEMEX_API_KEY="sk-..."
export LEMEX_BASE_URL="https://inference.rcp.epfl.ch/v1"
lemex
```

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

## Docs

- [**Installing & building**](./docs/install.md)
- [**Contributing**](./docs/contributing.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).
