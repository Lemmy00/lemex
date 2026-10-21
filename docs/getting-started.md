# Getting started with Lemex

Install Lemex using the [installation guide](install.md), then configure your API key:

```sh
lemex setup
lemex
```

Setup defaults to RCP and lets you choose another Responses API endpoint. See [API setup](authentication.md) for saved credentials and environment variables.

Start Lemex in your project directory and describe the task. Use `/model` to choose a model, `/permissions` to adjust approvals, and `/status` to see session settings and token usage. Type `/` to browse available [slash commands](slash_commands.md).

For a single task from your project's Git repository:

```sh
lemex exec "Explain this codebase"
```

Outside a Git repository, add `--skip-git-repo-check`. See [Configuration](config.md) for model and provider settings.
