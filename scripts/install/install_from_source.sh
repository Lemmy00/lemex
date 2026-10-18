#!/usr/bin/env bash
#
# Build and install Lemex from source.
#
# Usage:
#   git clone --depth 1 https://github.com/Lemmy00/lemex.git
#   cd lemex
#   ./scripts/install/install_from_source.sh
#
# Environment:
#   LEMEX_BUILD               "release" (default) or "debug".
#   LEMEX_PYTHON              Python 3.11+ executable override (otherwise detected).
#   LEMEX_INSTALL_PREFIX      npm global prefix override; passed to npm --prefix.
#   LEMEX_SKIP_RUST_INSTALL   Set to 1 to skip the rustup installation check.
#   LEMEX_INSTALL_DEPS        Set to 0 to check dependencies without installing them.
#   LEMEX_DEPS_DIR            Cache for user-local native build dependencies.
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
LEMEX_PYTHON_BIN="${LEMEX_PYTHON:-python3}"
LEMEX_HOME_DIR="${LEMEX_HOME:-$HOME/.lemex}"
LEMEX_TEMP_BUILD_DIR=""
LEMEX_TEMP_PACKAGE_DIR=""
LEMEX_TEMP_NPM_DIR=""
LEMEX_TEMP_DEPS_DIR=""

cleanup() {
    # Packaging and npm's tarball cache belong to this invocation, including
    # when packing/installing fails or the user interrupts the installer.
    for temporary in "$LEMEX_TEMP_PACKAGE_DIR" "$LEMEX_TEMP_NPM_DIR" "$LEMEX_TEMP_DEPS_DIR"; do
        if [ -n "$temporary" ]; then
            rm -rf -- "$temporary"
        fi
    done
    cleanup_build
}

cleanup_build() {
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
    LEMEX_TEMP_BUILD_DIR=""
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

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

select_python() {
    local candidate
    if [ -n "${LEMEX_PYTHON:-}" ]; then
        LEMEX_PYTHON_BIN="$LEMEX_PYTHON"
        command_exists "$LEMEX_PYTHON" && \
            "$LEMEX_PYTHON" -c 'import sys; sys.exit(sys.version_info < (3, 11))' && return
        error "LEMEX_PYTHON must name a working Python 3.11+ executable."
    fi
    # macOS can provide an older system python3 alongside a supported version.
    for candidate in python3 python3.15 python3.14 python3.13 python3.12 python3.11; do
        if command_exists "$candidate" && \
            "$candidate" -c 'import sys; sys.exit(sys.version_info < (3, 11))'; then
            LEMEX_PYTHON_BIN="$candidate"
            return
        fi
    done
    error "Python 3.11+ is required. Install a current Python 3 or set LEMEX_PYTHON."
}

check_base_deps() {
    step "Checking base dependencies"
    case "$(uname -s)" in
        Linux|Darwin) ;;
        MINGW*|MSYS*|CYGWIN*)
            error "Source installation on Windows requires WSL2. Run 'wsl --install' in PowerShell, then run this installer inside WSL2."
            ;;
        *) error "Source installation supports macOS, Linux, and Windows through WSL2." ;;
    esac
    if ! command_exists git; then
        error "git is required but not installed."
    fi
    select_python
    if ! command_exists node || ! command_exists npm; then
        error "Node.js and npm are required but not installed."
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
    command_exists curl || error "curl is required to install Rust through rustup."

    printf "Rust toolchain not found. Installing via rustup...\n"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
    printf "Installed: %s\n" "$(rustc --version)"
}

prepare_build_deps() {
    step "Preparing native build dependencies"
    local target
    target="$(rustc -vV | sed -n 's|^host: ||p')"
    LEMEX_TEMP_DEPS_DIR="$(mktemp -d "${TMPDIR:-/tmp}/lemex-deps.XXXXXX")"
    "$LEMEX_PYTHON_BIN" "$REPO_ROOT/scripts/install/prepare_source_deps.py" \
        --target "$target" --env-file "$LEMEX_TEMP_DEPS_DIR/env.sh" || \
        error "Build dependencies could not be prepared. No Cargo build was started."
    # This file contains only shell-quoted exports generated by our helper.
    source "$LEMEX_TEMP_DEPS_DIR/env.sh"
    rm -rf -- "$LEMEX_TEMP_DEPS_DIR"
    LEMEX_TEMP_DEPS_DIR=""
}

prepare_install_prefix() {
    local prefix existing
    prefix="${LEMEX_INSTALL_PREFIX:-$(npm prefix -g)}"
    existing="$prefix/lib/node_modules"
    while [ ! -d "$existing" ]; do
        existing="$(dirname "$existing")"
    done
    if [ ! -w "$existing" ]; then
        if [ -n "${LEMEX_INSTALL_PREFIX:-}" ]; then
            error "LEMEX_INSTALL_PREFIX is not writable: $prefix. Choose a directory you own."
        fi
        export LEMEX_INSTALL_PREFIX="$HOME/.local"
        mkdir -p "$LEMEX_INSTALL_PREFIX"
        warn "npm's global prefix is not writable; installing into $LEMEX_INSTALL_PREFIX without sudo."
    fi
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
    if [ "${LEMEX_VENDORED_OPENSSL:-0}" = "1" ]; then
        build_binaries+=(--features codex-cli/vendored-openssl)
    fi
    local v8_paths
    v8_paths="$(CODEX_REPO_ROOT="$REPO_ROOT" "$LEMEX_PYTHON_BIN" - "$REPO_ROOT" "$HOST_TARGET" "$CARGO_TARGET_DIR/v8" <<'PY'
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

    # Stage only this host's runtime outside the checkout. Old vendor targets
    # must not become duplicate binaries in every subsequent npm install.
    LEMEX_TEMP_PACKAGE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/lemex-runtime.XXXXXX")"
    LEMEX_PACKAGE_DIR="$LEMEX_TEMP_PACKAGE_DIR/runtime"
    mkdir -p "$LEMEX_PACKAGE_DIR/bin"
    cp "$CODEX_CLI_DIR/package.json" "$LEMEX_PACKAGE_DIR/package.json"
    cp "$CODEX_CLI_DIR/bin/lemex.js" "$LEMEX_PACKAGE_DIR/bin/lemex.js"
    VENDOR_DIR="$LEMEX_PACKAGE_DIR/vendor/$HOST_TARGET/bin"
    mkdir -p "$VENDOR_DIR"
    cp "$BUILD_OUTPUT_DIR/lemex" "$VENDOR_DIR/lemex"
    cp "$BUILD_OUTPUT_DIR/lemex-code-mode-host" "$VENDOR_DIR/lemex-code-mode-host"
    case "$HOST_TARGET" in
        *-linux-*)
            mkdir -p "$LEMEX_PACKAGE_DIR/vendor/$HOST_TARGET/codex-resources"
            cp "$BUILD_OUTPUT_DIR/bwrap" "$LEMEX_PACKAGE_DIR/vendor/$HOST_TARGET/codex-resources/bwrap"
            ;;
    esac
    "$LEMEX_PYTHON_BIN" "$REPO_ROOT/scripts/install/prepare_source_package.py" \
        "$LEMEX_PACKAGE_DIR/vendor/$HOST_TARGET" \
        --target "$HOST_TARGET" --rg "${LEMEX_RG_BIN:-$(command -v rg)}"
    printf "Copied binary to %s\n" "$VENDOR_DIR/lemex"
}

install_npm_package() {
    step "Installing lemex npm package globally"
    cd "${LEMEX_PACKAGE_DIR:-$CODEX_CLI_DIR}"

    # Installing a directory globally makes a symlink to the checkout. Pack it
    # first so the installed runtime survives moving or deleting that checkout.
    LEMEX_TEMP_NPM_DIR="$(mktemp -d "${TMPDIR:-/tmp}/lemex-package.XXXXXX")"
    local package_dir="$LEMEX_TEMP_NPM_DIR"
    npm pack --cache "$package_dir/cache" --pack-destination "$package_dir" >/dev/null || {
        error "Failed to pack the Lemex runtime."
    }
    local packages=("$package_dir"/*.tgz)
    NPM_ARGS=(-g "${packages[0]}")
    if [ -n "${LEMEX_INSTALL_PREFIX:-}" ]; then
        NPM_ARGS+=(--prefix "$LEMEX_INSTALL_PREFIX")
    fi

    npm install --cache "$package_dir/cache" --no-audit --no-fund "${NPM_ARGS[@]}" || {
        error "Failed to install the Lemex runtime."
    }
    rm -rf -- "$package_dir"
    LEMEX_TEMP_NPM_DIR=""
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
        printf "Installed bundled provider settings and model catalog.\n"
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
    "$LEMEX_PYTHON_BIN" "$REPO_ROOT/scripts/install/import_remote_config.py" \
        "$import_dir" "$remote_path" "$LEMEX_HOME_DIR" "$HOME" || {
        rm -rf -- "$import_dir"
        error "Failed to validate or import the remote config."
    }
    rm -rf -- "$import_dir"

    printf "Copied config to %s\n" "$LEMEX_HOME_DIR"
}

print_next_steps() {
    step "Next steps"
    if [ -n "${LEMEX_INSTALL_PREFIX:-}" ]; then
        case ":$PATH:" in
            *":$LEMEX_INSTALL_PREFIX/bin:"*) ;;
            *) printf 'Add the installed command to PATH:\n  export PATH=%q:"$PATH"\n\n' "$LEMEX_INSTALL_PREFIX/bin" ;;
        esac
    fi
    printf 'Configuration: %s/config.toml\nModel catalog: %s/models.json\n' \
        "$LEMEX_HOME_DIR" "$LEMEX_HOME_DIR"
    cat <<'EOF'

Set up your API key once and start Lemex:

  lemex setup
  lemex

Setup defaults to RCP and lets you change the API URL. No OpenAI login is needed.
For environment-only use, set LEMEX_API_KEY and optionally LEMEX_BASE_URL.
If you kept or imported a custom provider, set the key named by its env_key.
Run 'lemex doctor' to check your setup.
EOF
}

main() {
    check_base_deps
    install_rust
    prepare_build_deps
    prepare_install_prefix
    build_binary
    # Release the large compiler artifacts before npm creates its tarball and
    # installed runtime copies, keeping peak installation disk use lower.
    cleanup_build
    install_npm_package
    install_default_config
    copy_remote_config
    print_next_steps
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
