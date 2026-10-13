# Lemex app-server daemon

Lemex normally runs its backend inside the CLI process. The persistent daemon is
opt-in and supports shared sessions, background work, the agents overview, and
explicit remote control. No daemon updater or public-release downloader exists.

## Use

```sh
lemex --enable daemon_auto_start
lemex app-server daemon start
lemex app-server daemon restart
lemex app-server daemon stop
lemex app-server daemon version
lemex app-server daemon bootstrap --remote-control
lemex app-server daemon enable-remote-control
lemex app-server daemon disable-remote-control
lemex app-server daemon update
```

`--enable daemon_auto_start` applies to one launch. Use
`lemex features enable daemon_auto_start` to save the preference. Without that
opt-in, normal terminal and archive/unarchive commands neither discover nor attach
to an existing local daemon. `--no-daemon` always selects the embedded backend.
`lemex agents` explicitly requests a shared daemon. Explicit `--remote` connections
remain authoritative.

Lifecycle commands print one JSON object on success. `start` is idempotent and
returns after the server responds to initialization. `restart` stops the managed
server and starts it again. `bootstrap` configures remote control and starts it.
Mutating commands are serialized per `LEMEX_HOME`.

## Packages and updates

An explicit initial start copies the installed complete Lemex package into
`LEMEX_HOME/packages/app-server-daemon/releases/` and selects it through `current`.
The entrypoint is `bin/lemex` (`lemex.exe` on Windows); no duplicate Codex binaries
are needed. Older Codex-named packages remain readable for lifecycle management.
A bare executable cannot supply a complete new daemon package.

`daemon update` copies the currently installed Lemex package after confirmation,
and restarts only an already-running daemon. `--yes` confirms a foreground update;
`--from-cli` remains accepted for compatibility. No command downloads a Codex
release or restores automatic updates. Update Lemex through its source installer
first, then explicitly refresh the daemon if needed.

The daemon retains the environment of the invocation that started it. It cannot
adopt later terminals' environment changes. Normal embedded sessions use their
own invocation's environment. The daemon and its state are separate from the
installed CLI, so reinstalling the CLI alone does not refresh a running daemon.

## State and shutdown

State lives under `LEMEX_HOME/app-server-daemon/`: `settings.json`, `daemon.pid`,
`daemon.lock`, and diagnostic logs. The control socket lives under
`LEMEX_HOME/app-server-control/`. There is no launchd or systemd registration.

```json
{
  "remoteControlEnabled": false,
  "shutdownGraceSeconds": 60
}
```

Shutdown requests a graceful exit, then force-terminates after the configured
window (0 through 300 seconds). Stop, restart, and explicit package replacement
can interrupt active or queued work. Older `updater` settings are ignored.
Lifecycle operations retain identity-checked cleanup of legacy updater processes;
Lemex never launches one.

The daemon supports Linux, macOS, and Windows. Windows requires a non-elevated
terminal permitting detached child processes, and a control socket path that
fits the AF_UNIX limit.
