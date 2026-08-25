## Installing & building

### System requirements

| Requirement                 | Details                                                         |
| --------------------------- | --------------------------------------------------------------- |
| Operating systems           | macOS 12+, Ubuntu 20.04+/Debian 10+, or Windows 11 **via WSL2** |
| Git (optional, recommended) | 2.23+ for built-in PR helpers                                   |
| RAM                         | 4-GB minimum (8-GB recommended)                                 |

### From npm

Once `lemex` is published to npm, install it with:

```bash
npm install -g lemex
```

> **Note:** `lemex` is not yet published to npm. Build and install from source until the first release is available.

### DotSlash

Published GitHub Releases contain a [DotSlash](https://dotslash-cli.com/) file for the Lemex CLI named `lemex`. Using a DotSlash file makes it possible to make a lightweight commit to source control to ensure all contributors use the same version of an executable, regardless of what platform they use for development.

### Build from source (convenience script)

The fastest way to install from source is the provided script:

```bash
git clone https://github.com/Lemmy00/lemex.git
cd lemex
./scripts/install/install_from_source.sh
```

The script checks for Node.js/npm, installs Rust if needed, builds the release binary, installs `lemex` globally via npm, and copies the bundled default RCP config (`config/config.toml` and `config/models.json`) into `~/.lemex` if none exists.

To copy an existing Lemex config from another machine during install (overriding the default):

```bash
LEMEX_COPY_CONFIG_FROM=otherhost:/home/you/.lemex ./scripts/install/install_from_source.sh
```

### Build from source (manual)

```bash
# Clone the repository and navigate to the root of the Cargo workspace.
git clone https://github.com/Lemmy00/lemex.git
cd lemex/codex-rs

# Install the Rust toolchain, if necessary.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup component add rustfmt
rustup component add clippy
# Install helper tools used by the workspace justfile:
cargo install --locked just
# DotSlash fetches pinned development tools such as buildifier on first use.
cargo install --locked dotslash
# Install nextest for the `just test` helper.
cargo install --locked cargo-nextest

# Build the Lemex CLI binary. Use `-p codex-cli` to avoid compiling crates
# (such as the V8 proof-of-concept) that are not required by the CLI.
cargo build --release -p codex-cli

# Install the CLI globally via npm (optional). This places the `lemex`
# command on your PATH.
cp target/release/lemex ../codex-cli/vendor/$(rustc -vV | sed -n 's|host: ||p')/bin/lemex
cd ../codex-cli
npm install -g .

# Copy the bundled default RCP config if you do not have one yet.
mkdir -p "$HOME/.lemex"
[ -f "$HOME/.lemex/config.toml" ] || cp ../config/config.toml "$HOME/.lemex/config.toml"
[ -f "$HOME/.lemex/models.json" ] || cp ../config/models.json "$HOME/.lemex/models.json"

# Launch the TUI with a sample prompt.
cargo run --bin lemex -- "explain this codebase to me"

# After making changes, use the root justfile helpers (they default to codex-rs):
just fmt
just fix -p <crate-you-touched>

# Run the relevant tests (project-specific is fastest), for example:
just test -p codex-tui
# `just test` runs the test suite via nextest:
just test
# Avoid `--all-features` for routine local runs because it increases build
# time and `target/` disk usage by compiling additional feature combinations.
```

## Tracing / verbose logging

Lemex is written in Rust, so it honors the `RUST_LOG` environment variable to configure its logging behavior.

The TUI records diagnostics in bounded local stores by default. Set `log_dir` explicitly to enable a plaintext TUI log for a run:

```bash
lemex -c log_dir=./.lemex-log
tail -F ./.lemex-log/lemex-tui.log
```

The non-interactive mode (`lemex exec`) defaults to `RUST_LOG=error`, but messages are printed inline, so there is no need to monitor a separate file.

See the Rust documentation on [`RUST_LOG`](https://docs.rs/env_logger/latest/env_logger/#enabling-logging) for more information on the configuration options.
