## Sandbox and approvals

Lemex defaults to **Auto** where workspace sandboxing is available. In the
`/permissions` menu, select **Ask for approval** for this mode. Lemex can edit
workspace files and run commands. It asks before accessing the internet or
editing files outside the workspace.

Choose **Read Only** in `/permissions` for the current session. Set
`default_permissions = ":read-only"` in `~/.lemex/config.toml` to make it your
default. Folder trust controls access to project settings. Commands in a
folder marked untrusted require approval unless an explicit rule allows them.

For background on permission profiles and approvals, see the
[Codex security documentation](https://developers.openai.com/codex/security).
