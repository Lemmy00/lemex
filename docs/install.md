# Installation

## Requirements

Use macOS, Linux, or Windows through WSL2. Source installation needs:

- Git, curl, Node.js/npm, Python 3.11+, and ripgrep (`rg`).
- A C/C++ compiler and platform build dependencies.
- Rust; the installer installs it through rustup if it is missing.

On macOS, install the Xcode Command Line Tools (`xcode-select --install`). On Debian/Ubuntu, install the build dependencies with:

```sh
sudo apt install build-essential pkg-config libssl-dev libcap-dev
```

## Install from source

```sh
git clone https://github.com/Lemmy00/lemex.git
cd lemex
./scripts/install/install_from_source.sh
```

The installer builds the CLI and its helpers, downloads verified V8 artifacts, and installs the runtime globally through npm. The installed command keeps working if you move or remove the checkout. Ensure npm's global binary directory is on your `PATH`.

Missing `config.toml` and `models.json` files are copied from `config/` into `${LEMEX_HOME:-$HOME/.lemex}`. Existing files are left in place. Set `LEMEX_API_KEY` before starting Lemex; see [Configuration](config.md).

To update a source installation, pull the latest changes and run the installer again.

## Build options

No build environment variables are required. Common overrides are:

| Variable                  | Purpose                                                                       |
| ------------------------- | ----------------------------------------------------------------------------- |
| `LEMEX_BUILD`             | `release` (default) or `debug`.                                               |
| `LEMEX_INSTALL_PREFIX`    | Override npm's global installation prefix; add its `bin` directory to `PATH`. |
| `CARGO_BUILD_JOBS`        | Build concurrency; defaults to `4`. Lower it if memory is limited.            |
| `CARGO_TARGET_DIR`        | Reuse a build directory between installs.                                     |
| `LEMEX_KEEP_BUILD`        | Set to `1` to retain the generated temporary build directory.                 |
| `LEMEX_SKIP_RUST_INSTALL` | Set to `1` to require an existing Rust installation.                          |

By default, the temporary build directory is removed when installation finishes or fails. An explicitly supplied `CARGO_TARGET_DIR` is always retained:

```sh
CARGO_TARGET_DIR="$HOME/.cache/lemex-build" ./scripts/install/install_from_source.sh
```

## Import existing configuration

To copy configuration and the model catalog from another machine over SSH:

```sh
LEMEX_COPY_CONFIG_FROM=host:/home/user/.lemex ./scripts/install/install_from_source.sh
```

The import validates the catalog, adapts absolute home and configuration paths, and backs up replaced files as `*.bak` (with a numeric suffix if needed). Set the provider's API-key environment variable separately.

## Verify

```sh
lemex --version
lemex doctor
```

To check that your provider responds:

```sh
lemex exec --ephemeral --skip-git-repo-check "Reply exactly OK"
```
