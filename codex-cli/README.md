# Lemex CLI

Lemex is a local coding agent based on OpenAI Codex. It runs as `lemex` and stores its configuration in `~/.lemex`.

## Install

With Node.js 24 or later (Node.js 22 supports the core CLI):

```sh
npm install -g @lemmy00/lemex
lemex setup
lemex
```

Prebuilt packages support macOS and Linux on Intel/AMD and ARM64. On Windows, run these commands inside WSL2. npm installs only the native package for your computer; the core CLI does not require Rust, Python, or a compiler.

UltraCode is installed as an optional dependency and connected automatically for code search and analysis. Run `lemex-ultracode setup` to configure semantic search and embeddings. If UltraCode cannot be installed, the core CLI remains available. Use `LEMEX_DISABLE_ULTRACODE=1 lemex` to skip the bundled server. Existing custom UltraCode registrations take precedence.

`lemex setup` asks for an API key and endpoint. It installs the bundled configuration and model catalog without replacing existing files. The default endpoint is `https://inference.rcp.epfl.ch/v1`. You can use another compatible provider:

```sh
lemex setup --base-url https://api.example.com/v1 --model your-model-id
```

To run a single task from your project:

```sh
lemex exec "Explain this codebase"
```

Update with `npm install -g @lemmy00/lemex@latest`. Check your setup with `lemex doctor`.

See the [installation guide](https://github.com/Lemmy00/lemex/blob/main/docs/install.md) and [configuration guide](https://github.com/Lemmy00/lemex/blob/main/docs/config.md).

Lemex is licensed under Apache-2.0. The separate UltraCode dependency retains its AGPL-3.0 or commercial license. External analytics and feedback reporting are removed; local logs and token usage are preserved.
