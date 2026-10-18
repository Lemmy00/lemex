# Lemex CLI

Lemex is a local coding agent based on OpenAI Codex. It runs as `lemex` and stores its configuration in `~/.lemex`.

## Install

With Node.js 22 or later:

```sh
npm install -g lemex
lemex setup
lemex
```

Prebuilt packages support macOS and Linux on Intel/AMD and ARM64. On Windows, run these commands inside WSL2. npm installs only the native package for your computer; Rust, Python, and a compiler are not required.

`lemex setup` asks for an API key and endpoint. It installs the bundled configuration and model catalog without replacing existing files. The default endpoint is `https://inference.rcp.epfl.ch/v1`. You can use another compatible provider:

```sh
lemex setup --base-url https://api.example.com/v1 --model your-model-id
```

To run a single task from your project:

```sh
lemex exec "Explain this codebase"
```

Update with `npm install -g lemex@latest`. Check your setup with `lemex doctor`.

See the [installation guide](https://github.com/Lemmy00/lemex/blob/main/docs/install.md) and [configuration guide](https://github.com/Lemmy00/lemex/blob/main/docs/config.md).

Licensed under Apache-2.0. External analytics and feedback reporting are removed; local logs and token usage are preserved.
