#!/usr/bin/env bash
#
# Build and install Lemex from source.
#
# Usage:
#   git clone https://github.com/Lemmy00/lemex.git
#   cd lemex
#   ./scripts/install/install_from_source.sh
#
# Environment:
#   LEMEX_BUILD               "release" (default) or "debug".
#   LEMEX_INSTALL_PREFIX      npm global prefix override; passed to npm --prefix.
#   LEMEX_SKIP_RUST_INSTALL   Set to 1 to skip the rustup installation check.
#   LEMEX_COPY_CONFIG_FROM    ssh host:path to copy an existing ~/.lemex config from.
#   LEMEX_KEEP_BUILD          Set to 1 to retain the temporary Cargo build directory.
#   CARGO_TARGET_DIR          Reuse an existing build directory; it is never deleted.
#   CARGO_BUILD_JOBS          Parallel build jobs (default: 4).
#   CARGO_PROFILE_RELEASE_LTO Source-install LTO override (default: off).
#   CARGO_PROFILE_RELEASE_CODEGEN_UNITS Codegen parallelism (default: 16).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODEX_RS_DIR="$REPO_ROOT/codex-rs"
CODEX_CLI_DIR="$REPO_ROOT/codex-cli"

LEMEX_BUILD="${LEMEX_BUILD:-release}"
LEMEX_SKIP_RUST_INSTALL="${LEMEX_SKIP_RUST_INSTALL:-0}"
LEMEX_HOME_DIR="${LEMEX_HOME:-$HOME/.lemex}"
LEMEX_TEMP_BUILD_DIR=""

cleanup() {
    if [ -z "$LEMEX_TEMP_BUILD_DIR" ]; then
        return
    fi
    if [ "${LEMEX_KEEP_BUILD:-0}" = "1" ]; then
        printf '\nKept Cargo build cache: %s\n' "$LEMEX_TEMP_BUILD_DIR"
        printf 'Reuse it on the next install with CARGO_TARGET_DIR set to that path.\n'
    else
        printf '\nRemoving temporary Cargo build cache: %s\n' "$LEMEX_TEMP_BUILD_DIR"
        rm -rf -- "$LEMEX_TEMP_BUILD_DIR"
    fi
}

trap cleanup EXIT

step() {
    printf '\n==> %s\n' "$1"
}

warn() {
    printf 'WARNING: %s\n' "$1" >&2
}

error() {
    printf 'ERROR: %s\n' "$1" >&2
    exit 1
}

command_exists() {
    command -v "$1" >/dev/null 2>&1
}

check_base_deps() {
    step "Checking base dependencies"
    if ! command_exists git; then
        error "git is required but not installed."
    fi
    if ! command_exists python3; then
        error "Python 3 is required to fetch the verified V8 build artifacts."
    fi
    if ! command_exists node || ! command_exists npm; then
        error "Node.js and npm are required but not installed."
    fi
    if ! command_exists rg; then
        error "ripgrep (rg) is required for the complete CLI runtime package."
    fi
}

install_rust() {
    step "Checking Rust toolchain"
    if command_exists cargo && command_exists rustc; then
        printf "Rust toolchain already available: %s\n" "$(rustc --version)"
        return
    fi

    if [ "$LEMEX_SKIP_RUST_INSTALL" = "1" ]; then
        error "Rust toolchain not found and LEMEX_SKIP_RUST_INSTALL=1."
    fi

    printf "Rust toolchain not found. Installing via rustup...\n"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
    printf "Installed: %s\n" "$(rustc --version)"
}

build_binary() {
    case "$LEMEX_BUILD" in
        release|debug) ;;
        *) error "Unknown LEMEX_BUILD value: $LEMEX_BUILD. Use 'release' or 'debug'." ;;
    esac
    step "Building Lemex CLI ($LEMEX_BUILD)"
    cd "$CODEX_RS_DIR"

    if [ -z "${CARGO_TARGET_DIR:-}" ]; then
        local cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}"
        mkdir -p "$cache_dir"
        LEMEX_TEMP_BUILD_DIR="$(mktemp -d "$cache_dir/lemex-build.XXXXXX")"
        export CARGO_TARGET_DIR="$LEMEX_TEMP_BUILD_DIR"
    elif [[ "$CARGO_TARGET_DIR" != /* ]]; then
        export CARGO_TARGET_DIR="$CODEX_RS_DIR/$CARGO_TARGET_DIR"
    fi
    printf 'Cargo build cache: %s\n' "$CARGO_TARGET_DIR"
    if [ -z "$LEMEX_TEMP_BUILD_DIR" ]; then
        printf 'Caller-supplied CARGO_TARGET_DIR will be retained.\n'
    fi
    export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
    export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
    export STABLE_GIT_COMMIT="${STABLE_GIT_COMMIT:-$(git -C "$REPO_ROOT" rev-parse HEAD)}"

    HOST_TARGET=$(rustc -vV | sed -n 's|^host: ||p')
    local build_binaries=(--bin lemex --bin lemex-code-mode-host)
    case "$HOST_TARGET" in
        *-linux-*) build_binaries+=(--bin bwrap) ;;
    esac
    local v8_paths
    v8_paths="$(CODEX_REPO_ROOT="$REPO_ROOT" python3 - "$REPO_ROOT" "$HOST_TARGET" "$CARGO_TARGET_DIR/v8" <<'PY'
import sys
from pathlib import Path

sys.path.insert(0, str(Path(sys.argv[1]) / "scripts"))
from codex_package.targets import TARGET_SPECS
from codex_package.v8 import resolve_codex_v8_cargo_env

env = resolve_codex_v8_cargo_env(
    TARGET_SPECS[sys.argv[2]], cache_root=Path(sys.argv[3])
)
if env:
    print(env["RUSTY_V8_ARCHIVE"])
    print(env["RUSTY_V8_SRC_BINDING_PATH"])
PY
    )" || error "Failed to fetch the verified V8 build artifacts."
    if [ -n "$v8_paths" ]; then
        # macOS ships Bash 3.2, which has no mapfile builtin.
        local v8_artifacts=()
        local v8_artifact
        while IFS= read -r v8_artifact; do
            v8_artifacts+=("$v8_artifact")
        done <<< "$v8_paths"
        export RUSTY_V8_ARCHIVE="${v8_artifacts[0]}"
        export RUSTY_V8_SRC_BINDING_PATH="${v8_artifacts[1]}"
    fi

    case "$LEMEX_BUILD" in
        release)
            export CARGO_PROFILE_RELEASE_DEBUG="${CARGO_PROFILE_RELEASE_DEBUG:-0}"
            export CARGO_PROFILE_RELEASE_STRIP="${CARGO_PROFILE_RELEASE_STRIP:-symbols}"
            # Keep optimization, but avoid release-distribution link settings
            # that make a one-off local installation much slower.
            export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-off}"
            export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-16}"
            cargo build --locked --release "${build_binaries[@]}"
            BUILD_OUTPUT_DIR="$CARGO_TARGET_DIR/release"
            ;;
        debug)
            cargo build --locked --profile dev-small "${build_binaries[@]}"
            BUILD_OUTPUT_DIR="$CARGO_TARGET_DIR/dev-small"
            ;;
    esac

    VENDOR_DIR="$CODEX_CLI_DIR/vendor/$HOST_TARGET/bin"
    mkdir -p "$VENDOR_DIR"
    cp "$BUILD_OUTPUT_DIR/lemex" "$VENDOR_DIR/lemex"
    cp "$BUILD_OUTPUT_DIR/lemex-code-mode-host" "$VENDOR_DIR/lemex-code-mode-host"
    case "$HOST_TARGET" in
        *-linux-*)
            mkdir -p "$CODEX_CLI_DIR/vendor/$HOST_TARGET/codex-resources"
            cp "$BUILD_OUTPUT_DIR/bwrap" "$CODEX_CLI_DIR/vendor/$HOST_TARGET/codex-resources/bwrap"
            ;;
    esac
    python3 "$REPO_ROOT/scripts/install/prepare_source_package.py" \
        "$CODEX_CLI_DIR/vendor/$HOST_TARGET" \
        --target "$HOST_TARGET" --rg "$(command -v rg)"
    printf "Copied binary to %s\n" "$VENDOR_DIR/lemex"
}

install_npm_package() {
    step "Installing lemex npm package globally"
    cd "$CODEX_CLI_DIR"

    # Installing a directory globally makes a symlink to the checkout. Pack it
    # first so the installed runtime survives moving or deleting that checkout.
    local package_dir
    package_dir="$(mktemp -d "${TMPDIR:-/tmp}/lemex-package.XXXXXX")"
    npm pack --pack-destination "$package_dir" >/dev/null || {
        rm -rf -- "$package_dir"
        error "Failed to pack the Lemex runtime."
    }
    local packages=("$package_dir"/*.tgz)
    NPM_ARGS=(-g "${packages[0]}")
    if [ -n "${LEMEX_INSTALL_PREFIX:-}" ]; then
        NPM_ARGS+=(--prefix "$LEMEX_INSTALL_PREFIX")
    fi

    npm install "${NPM_ARGS[@]}" || {
        rm -rf -- "$package_dir"
        error "Failed to install the Lemex runtime."
    }
    rm -rf -- "$package_dir"
}

install_default_config() {
    step "Installing default Lemex config"
    mkdir -p "$LEMEX_HOME_DIR"

    local copied_something=false
    if [ ! -f "$LEMEX_HOME_DIR/config.toml" ]; then
        cp "$REPO_ROOT/config/config.toml" "$LEMEX_HOME_DIR/config.toml"
        printf "Installed %s\n" "$LEMEX_HOME_DIR/config.toml"
        copied_something=true
    else
        printf "Skipped %s (already exists)\n" "$LEMEX_HOME_DIR/config.toml"
    fi

    if [ ! -f "$LEMEX_HOME_DIR/models.json" ]; then
        cp "$REPO_ROOT/config/models.json" "$LEMEX_HOME_DIR/models.json"
        printf "Installed %s\n" "$LEMEX_HOME_DIR/models.json"
        copied_something=true
    else
        printf "Skipped %s (already exists)\n" "$LEMEX_HOME_DIR/models.json"
    fi

    if [ "$copied_something" = true ]; then
        printf "Default config uses the RCP provider with moonshotai/Kimi-K2.7-Code.\n"
    fi
}

copy_remote_config() {
    if [ -z "${LEMEX_COPY_CONFIG_FROM:-}" ]; then
        return
    fi

    step "Copying Lemex config from $LEMEX_COPY_CONFIG_FROM"
    mkdir -p "$LEMEX_HOME_DIR"

    # Expected format: host:/path/to/.lemex
    remote_path="${LEMEX_COPY_CONFIG_FROM#*:}"

    local import_dir
    import_dir="$(mktemp -d "$LEMEX_HOME_DIR/import.XXXXXX")"
    scp "$LEMEX_COPY_CONFIG_FROM/config.toml" "$import_dir/config.toml" || {
        rm -rf -- "$import_dir"
        error "Failed to copy config.toml"
    }
    scp "$LEMEX_COPY_CONFIG_FROM/models.json" "$import_dir/models.json" || {
        rm -rf -- "$import_dir"
        error "Failed to copy models.json"
    }
    python3 "$REPO_ROOT/scripts/install/import_remote_config.py" \
        "$import_dir" "$remote_path" "$LEMEX_HOME_DIR" "$HOME" || {
        rm -rf -- "$import_dir"
        error "Failed to validate or import the remote config."
    }
    rm -rf -- "$import_dir"

    printf "Copied config to %s\n" "$LEMEX_HOME_DIR"
}

print_next_steps() {
    step "Next steps"
    cat <<'EOF'
Lemex is installed and a default RCP config was placed in ~/.lemex.

Before using it, configure your environment:

  export LEMEX_API_KEY="sk-..."
  export LEMEX_BASE_URL="https://inference.rcp.epfl.ch/v1"  # optional; this is the default

Add the exports to your shell profile (e.g. ~/.zshrc) to make them persistent:

  cat >> ~/.zshrc <<'VARS'
  export LEMEX_API_KEY="sk-..."
  export LEMEX_BASE_URL="https://inference.rcp.epfl.ch/v1"
  VARS
  source ~/.zshrc

If you already have a Lemex config on another machine, copy it over:

  scp otherhost:/home/you/.lemex/config.toml ~/.lemex/config.toml
  scp otherhost:/home/you/.lemex/models.json ~/.lemex/models.json
  sed -i.bak "s|/home/you|$HOME|g" ~/.lemex/config.toml

Run a quick health check:

  lemex doctor
  lemex --version

Start the TUI:

  lemex
EOF
}

main() {
    check_base_deps
    install_rust
    build_binary
    install_npm_package
    install_default_config
    copy_remote_config
    print_next_steps
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
