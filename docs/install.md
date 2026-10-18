# Installation

## Requirements

Use Node.js 22+ and npm on macOS or Linux (x64 or ARM64). On Windows, install Node.js inside WSL2 and run Lemex there.

## Install from npm

> The first npm release is being prepared. Until it is published, use [installation from source](#install-from-source).

```sh
npm install -g @lemmy00/lemex
lemex setup
lemex
```

npm downloads the precompiled runtime for your platform, including the code-mode host, ripgrep, and the Linux sandbox helper. Rust, Python, a compiler, and a source checkout are not required. `lemex setup` installs the bundled provider settings and model catalog in `${LEMEX_HOME:-$HOME/.lemex}`, preserving existing files, then helps configure your API key and model.

If you previously installed the unscoped `lemex` package from source, remove it with `npm uninstall -g lemex` before installing `@lemmy00/lemex`. Your settings in `~/.lemex` are retained.

To update:

```sh
npm install -g @lemmy00/lemex@latest
```

If your npm global installation directory is not writable, use a user-owned prefix:

```sh
npm install -g @lemmy00/lemex --prefix "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
```

Add that PATH setting to your shell profile to keep it for future terminals. See [API setup](authentication.md) for providers and credentials.

## Source build requirements

Building from source additionally needs:

- Git, curl, Node.js/npm, and Python 3.11+.
- A C/C++ compiler and platform build dependencies.
- Rust; the installer installs it through rustup if it is missing.

On macOS, install the Xcode Command Line Tools (`xcode-select --install`) and finish the installation before running the source installer. Linux builds need a C/C++ toolchain, make, pkg-config, Perl, and binutils. The installer checks these before starting Cargo and installs missing build dependencies through apt, dnf/yum, zypper, pacman, or apk when it can run the package manager as root or with passwordless sudo. It never prompts for a sudo password.

Without sudo, the installer builds a checksum-verified static libcap in your user cache and uses Cargo's bundled OpenSSL when its development files are missing. The resulting sandbox does not depend on the cached libcap at runtime. An existing `PKG_CONFIG_PATH` is preserved; no shell-profile edits or manual environment setup are required. If ripgrep (`rg`) is missing, the installer fetches the verified binary for your platform automatically.

To provision Debian/Ubuntu build dependencies yourself:

```sh
sudo apt install build-essential pkg-config perl libssl-dev libcap-dev
```

## Install from source

```sh
git clone --depth 1 https://github.com/Lemmy00/lemex.git
cd lemex
./scripts/install/install_from_source.sh
```

The installer builds the CLI and its helpers, downloads verified V8 artifacts, and installs the runtime globally through npm. If npm's default global prefix is not writable, it uses `~/.local` without sudo. The installed command keeps working if you move or remove the checkout. Ensure the displayed installation directory is on your `PATH`.

On Windows, run the source installer in WSL2 (install it with `wsl --install` in PowerShell). Git Bash, MSYS, and Cygwin are detected early and directed to WSL2.

Source builds compile the Rust workspace locally and take longer than the npm installation. Lemex's release installers (`scripts/install/install.sh` and `scripts/install/install.ps1`) expect separate GitHub binary-release assets; use npm or the source installer for the supported installation paths. A shallow clone avoids downloading the full upstream Git history.

Missing `config.toml` and `models.json` files are copied from `config/` into `${LEMEX_HOME:-$HOME/.lemex}`. Existing files are left in place. Run `lemex setup` to save your API key and endpoint before starting Lemex; see [API setup](authentication.md).

To update a source installation, pull the latest changes and run the installer again.

## Build options

No build environment variables are required. Common overrides are:

| Variable                  | Purpose                                                                                   |
| ------------------------- | ----------------------------------------------------------------------------------------- |
| `LEMEX_BUILD`             | `release` (default) or `debug`.                                                           |
| `LEMEX_PYTHON`            | Select a Python 3.11+ executable; supported versions are detected automatically.          |
| `LEMEX_INSTALL_PREFIX`    | Override npm's global installation prefix; add its `bin` directory to `PATH`.             |
| `CARGO_BUILD_JOBS`        | Build concurrency; defaults to `4`. Lower it if memory is limited.                        |
| `CARGO_TARGET_DIR`        | Reuse a build directory between installs.                                                 |
| `LEMEX_KEEP_BUILD`        | Set to `1` to retain the generated temporary build directory.                             |
| `LEMEX_SKIP_RUST_INSTALL` | Set to `1` to require an existing Rust installation.                                      |
| `LEMEX_INSTALL_DEPS`      | Set to `0` to check dependencies without installing or downloading missing ones.          |
| `LEMEX_DEPS_DIR`          | Override the native dependency cache (default: `${XDG_CACHE_HOME:-~/.cache}/lemex/deps`). |

Source installs disable incremental compilation and release debug symbols to reduce disk usage. Build artifacts, runtime staging, tarballs, and the install's npm cache are temporary and removed on success, failure, or interruption. The installed runtime and reusable dependency caches are retained; Cargo's shared dependency downloads remain available for other Rust projects. An explicitly supplied `CARGO_TARGET_DIR` is always retained:

```sh
CARGO_TARGET_DIR="$HOME/.cache/lemex-build" ./scripts/install/install_from_source.sh
```

Development builds also disable incremental compilation by default; set `CARGO_INCREMENTAL=1` to opt in. To reclaim build artifacts from earlier development builds, run `just clean-build` (or `cd codex-rs && cargo clean`). This leaves the globally installed CLI and your configuration intact. Source builds still need temporary disk space while compiling.

## Import existing configuration

To copy configuration and the model catalog from another machine over SSH:

```sh
LEMEX_COPY_CONFIG_FROM=host:/home/user/.lemex ./scripts/install/install_from_source.sh
```

The import validates the catalog, adapts absolute home and configuration paths, and backs up replaced files as `*.bak` (with a numeric suffix if needed). Run `lemex setup` for the built-in RCP provider, or set the custom provider's API-key environment variable separately.

## Verify

```sh
lemex --version
lemex doctor
```

To check that your provider responds:

```sh
lemex exec --ephemeral --skip-git-repo-check "Reply exactly OK"
```
