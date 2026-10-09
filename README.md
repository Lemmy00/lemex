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

The script checks for Node.js/npm, installs the Rust toolchain if necessary, builds the CLI, installs the `lemex` command globally via npm, and copies a default RCP config (`config/config.toml` and `config/models.json`) into `~/.lemex` if none exists. The installed runtime is copied from a package archive, so moving or deleting the source checkout does not break the command.

To also copy an existing Lemex config from another machine (overriding the default):

```bash
LEMEX_COPY_CONFIG_FROM=otherhost:/home/you/.lemex ./scripts/install/install_from_source.sh
```

For the setup on `larapc2`:

```bash
LEMEX_COPY_CONFIG_FROM=larapc2:/home/milikic/.lemex ./scripts/install/install_from_source.sh
```

The import preserves the server's model catalog, relocates absolute config and
home paths to this machine, and backs up existing settings as `*.bak` (with a
numeric suffix when needed). It imports configuration and the catalog only;
configure the API-key environment variable separately. Both downloads and
catalog validation must succeed before existing settings are replaced.

If an older installation reports `command not found` or `no such file or directory`,
check `ls -l "$(npm root -g)/lemex"`. A link to a missing checkout, especially
one under `/tmp` or a macOS temporary directory, requires reinstalling with the
updated source installer above.

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

### Thinking traces

The default RCP config enables full thinking traces returned by the provider:

```toml
show_raw_agent_reasoning = true
```

Press **Ctrl+T** to open the expanded transcript and see completed thinking
blocks. Press **Ctrl+T** again or **q** to return to the compact view.
While a block is still being generated, the status line shows its latest
reasoning activity; the full block appears in the transcript when it completes.

To disable raw thinking text, set `show_raw_agent_reasoning = false` in
`~/.lemex/config.toml`, or override it for one launch:

```shell
lemex -c show_raw_agent_reasoning=false
```

### Web search with RCP models

RCP provides model inference but does not provide OpenAI's hosted web-search
service. Lemex's default RCP config uses the hosted [Exa MCP server](https://exa.ai/docs/get-started/exa-mcp)
for web search, page fetching, and searches with domain or date filters. This
works independently of the inference provider and needs no additional API key
to get started. Keyless use is free and rate limited.

For an existing installation, add the server:

```shell
lemex mcp add exa --url 'https://mcp.exa.ai/mcp?tools=web_search_exa,web_fetch_exa,web_search_advanced_exa'
```

Add these **top-level** settings in `~/.lemex/config.toml`, before any `[table]`:

```toml
# Disable the unsupported hosted service. MCP web tools remain available.
web_search = "disabled"
developer_instructions = """
Use the Exa MCP web tools when information may have changed, when a fact is uncertain,
or when the user asks to search or verify. Search first, then fetch relevant source
pages before answering. Prefer official documentation for technical questions and
use advanced search for domain or date filters. Cite source URLs in the answer.
If the web tools fail, report the failure and distinguish unverified knowledge
from retrieved evidence.
"""
```

If you already have `developer_instructions`, append the web guidance to it.
Keep `supports_standalone_web_search = false` for RCP: that setting describes
the provider's search endpoint, not MCP support. If you switch to an OpenAI
provider, you can enable its native search with `web_search = "live"`.

Start a new Lemex session and use `/mcp` to check that `exa` exposes
`web_search_exa`, `web_fetch_exa`, and `web_search_advanced_exa`. A configured
server is not enough to prove retrieval works; verify both search and fetch:

```shell
lemex exec --ephemeral --json 'Use Exa to search for the official Python tomllib documentation, fetch that page, and report which Python version introduced tomllib. Cite the fetched source. Do not use shell commands or answer from memory.'
```

Look for completed `mcp_tool_call` events for search and fetch with no error,
and a final answer grounded in the returned page. If keyless usage hits a rate
limit, configure your own Exa key or another search MCP server; access to the
internet alone does not supply a search backend.

### Health check

```shell
lemex doctor
lemex --version
```

## Docs

- [**Installing & building**](./docs/install.md)
- [**Contributing**](./docs/contributing.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).
