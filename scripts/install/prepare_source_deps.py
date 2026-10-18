#!/usr/bin/env python3
"""Prepare native source-build dependencies before invoking Cargo."""

import argparse
import hashlib
import os
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path
from urllib.request import urlopen


ROOT = Path(__file__).resolve().parents[2]
LIBCAP_VERSION = "2.78"
LIBCAP_SHA256 = "2a2c705e382c413643a458b837575c0eb0989477ab6fb99c87adbe9a259612ad"
LIBCAP_URL = (
    "https://www.kernel.org/pub/linux/libs/security/linux-privs/libcap2/"
    f"libcap-{LIBCAP_VERSION}.tar.gz"
)
# These are build tools, not runtime dependencies of the installed CLI.
LINUX_PACKAGES = {
    "apt-get": ["build-essential", "pkg-config", "perl", "libcap-dev", "libssl-dev"],
    "dnf": ["gcc", "gcc-c++", "make", "pkgconf-pkg-config", "perl", "libcap-devel", "openssl-devel"],
    "yum": ["gcc", "gcc-c++", "make", "pkgconfig", "perl", "libcap-devel", "openssl-devel"],
    "zypper": ["gcc", "gcc-c++", "make", "pkg-config", "perl", "libcap-devel", "libopenssl-devel"],
    "pacman": ["base-devel", "pkgconf", "perl", "libcap", "openssl"],
    "apk": ["build-base", "linux-headers", "pkgconf", "perl", "libcap-dev", "openssl-dev"],
}


def log(message: str) -> None:
    print(message, file=sys.stderr, flush=True)


def run(args: list[str], *, env: dict[str, str] | None = None, **kwargs):
    # Keep stdout free for the shell environment returned to the caller.
    return subprocess.run(args, env=env, stdout=sys.stderr, check=True, **kwargs)


def package_command(manager: str, packages: list[str]) -> list[str]:
    options = {
        "apt-get": ["install", "-y", "--no-install-recommends"],
        "dnf": ["install", "-y"],
        "yum": ["install", "-y"],
        "zypper": ["--non-interactive", "install"],
        "pacman": ["-S", "--needed", "--noconfirm"],
        "apk": ["add", "--no-cache"],
    }
    return [manager, *options[manager], *packages]


def system_packages(platform: str) -> bool:
    if platform == "macos":
        # Apple's SDK installation is interactive and cannot be replaced by
        # installing a compiler formula. Give the Xcode instruction below.
        return False
    manager = next((name for name in LINUX_PACKAGES if shutil.which(name)), None)
    if manager is None:
        return False
    privilege = []
    if os.geteuid() != 0:
        if not shutil.which("sudo") or subprocess.run(
            ["sudo", "-n", "true"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        ).returncode:
            return False
        privilege = ["sudo", "-n"]
    try:
        if manager == "apt-get":
            run([*privilege, manager, "update"])
        run([*privilege, *package_command(manager, LINUX_PACKAGES[manager])])
    except subprocess.CalledProcessError:
        log("System package installation failed; checking user-directory alternatives.")
        return False
    return True


def build_tools(platform: str) -> list[str]:
    required = ["make"]
    for variable, default in (("CC", "cc"), ("CXX", "c++")):
        command = shlex.split(os.environ.get(variable) or default)
        required.append(command[0])
    if platform == "linux":
        required += ["pkg-config", "perl", "ar", "ranlib"]
    return [name for name in required if not shutil.which(name)]


def compiler_check(env: dict[str, str]) -> None:
    # Xcode's cc/c++ shims can exist even when its SDK is not installed.
    with tempfile.TemporaryDirectory(prefix="lemex-compiler-check-") as temporary:
        for variable, compiler, language in (("CC", "cc", "c"), ("CXX", "c++", "c++")):
            command = shlex.split(env.get(variable) or compiler)
            result = subprocess.run(
                [*command, "-x", language, "-", "-o", str(Path(temporary) / "check")],
                input="int main(void) { return 0; }\n", text=True,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env,
            )
            if result.returncode:
                raise RuntimeError(f"The {language} compiler cannot link a program:\n{result.stderr}")


def library_available(name: str, env: dict[str, str]) -> bool:
    return bool(shutil.which("pkg-config")) and subprocess.run(
        ["pkg-config", "--exists", name], env=env,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ).returncode == 0


def libcap_check(env: dict[str, str]) -> bool:
    if not library_available("libcap", env):
        return False
    flags = subprocess.check_output(
        ["pkg-config", "--cflags", "--libs", "libcap"], env=env, text=True
    )
    with tempfile.TemporaryDirectory(prefix="lemex-libcap-check-") as temporary:
        result = subprocess.run(
            [*shlex.split(env.get("CC") or "cc"), "-x", "c", "-",
             "-o", str(Path(temporary) / "check"), *shlex.split(flags)],
            input="#include <sys/capability.h>\nint main(void) { cap_t c = cap_get_proc(); return cap_free(c); }\n",
            text=True, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        return result.returncode == 0


def prepare_libcap(prefix: Path, env: dict[str, str]) -> None:
    """Build only the small static library; no privileged install or runtime PATH."""
    library = prefix / "lib/libcap.a"
    header = prefix / "include/sys/capability.h"
    if not library.is_file() or not header.is_file():
        log(f"Building libcap {LIBCAP_VERSION} in the user cache (no sudo needed).")
        with tempfile.TemporaryDirectory(prefix="lemex-libcap-") as temporary:
            workspace = Path(temporary)
            archive_path = workspace / "libcap.tar.gz"
            with urlopen(LIBCAP_URL, timeout=60) as response:
                archive_path.write_bytes(response.read())
            digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
            if digest != LIBCAP_SHA256:
                raise RuntimeError("libcap source archive failed SHA-256 verification")
            with tarfile.open(archive_path) as archive:
                archive.extractall(workspace, filter="data")
            source = workspace / f"libcap-{LIBCAP_VERSION}"
            run([
                "make", "-s", "-C", str(source / "libcap"), "libcap.a",
                "lib=lib", "SHARED=no", "PTHREADS=no", "GOLANG=no", "PAM_CAP=no",
                "USE_GPERF=no", f"CC={env.get('CC') or 'cc'}",
                f"BUILD_CC={env.get('CC') or 'cc'}",
            ], env=env)
            library.parent.mkdir(parents=True, exist_ok=True)
            header.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source / "libcap/libcap.a", library)
            shutil.copy2(source / "libcap/include/sys/capability.h", header)
            shutil.copy2(source / "License", prefix / "LICENSE.libcap")
    pc = prefix / "lib/pkgconfig/libcap.pc"
    pc.parent.mkdir(parents=True, exist_ok=True)
    pc.write_text(
        f"prefix={prefix}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n\n"
        "Name: libcap\nDescription: Linux capabilities library\n"
        f"Version: {LIBCAP_VERSION}\nLibs: -L${{libdir}} -lcap\nCflags: -I${{includedir}}\n"
    )


def prepare(target: str, *, install: bool = True) -> dict[str, str]:
    if "windows" in target:
        raise RuntimeError("Source installation on Windows requires WSL2. Run this installer inside WSL2.")
    if "apple-darwin" in target:
        platform = "macos"
    elif "linux" in target:
        platform = "linux"
    else:
        raise RuntimeError(f"Unsupported source-build target: {target}")
    os.environ["CODEX_REPO_ROOT"] = str(ROOT)
    sys.path.insert(0, str(ROOT / "scripts"))
    from codex_package.targets import TARGET_SPECS

    if target not in TARGET_SPECS:
        raise RuntimeError(f"Unsupported source-build target: {target}. Use an x86-64 or ARM64 host.")
    env = dict(os.environ)
    missing = build_tools(platform)
    needs_libraries = platform == "linux" and (
        not libcap_check(env) or ("musl" not in target and not library_available("openssl", env))
    )
    if install and (missing or needs_libraries):
        system_packages(platform)
        missing = build_tools(platform)
    if missing:
        if platform == "macos":
            hint = "Install the Xcode Command Line Tools with 'xcode-select --install', then rerun."
        else:
            manager = next((name for name in LINUX_PACKAGES if shutil.which(name)), None)
            hint = (
                "Install build prerequisites with: sudo " + shlex.join(package_command(manager, LINUX_PACKAGES[manager]))
                if manager else "Install a C/C++ toolchain, make, pkg-config, Perl, and binutils for your distribution."
            )
        raise RuntimeError(f"Missing build tools: {', '.join(missing)}. {hint}")
    try:
        compiler_check(env)
    except RuntimeError as exc:
        if platform == "macos":
            raise RuntimeError(f"{exc}\nInstall the Xcode Command Line Tools with 'xcode-select --install'.") from exc
        raise

    exports = {"LEMEX_VENDORED_OPENSSL": "0"}
    if platform == "linux":
        if not libcap_check(env):
            if not install:
                raise RuntimeError("libcap headers and library are missing (libcap-dev / libcap-devel).")
            cache = Path(env.get("LEMEX_DEPS_DIR") or Path(env.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "lemex/deps")
            prefix = cache.expanduser().resolve() / target / f"libcap-{LIBCAP_VERSION}"
            prepare_libcap(prefix, env)
            env["PKG_CONFIG_PATH"] = str(prefix / "lib/pkgconfig") + (
                os.pathsep + env["PKG_CONFIG_PATH"] if env.get("PKG_CONFIG_PATH") else ""
            )
            # The installed sandbox must not need a library from the build cache.
            env["LIBCAP_STATIC"] = "1"
            exports.update(PKG_CONFIG_PATH=env["PKG_CONFIG_PATH"], LIBCAP_STATIC="1")
            if not libcap_check(env):
                raise RuntimeError("The locally built libcap cannot be linked by the C compiler.")
        if "musl" not in target and not library_available("openssl", env):
            if not install:
                raise RuntimeError("OpenSSL development files are missing (libssl-dev / openssl-devel).")
            log("OpenSSL development files are missing; Cargo will build its bundled OpenSSL.")
            exports["LEMEX_VENDORED_OPENSSL"] = "1"

    rg = shutil.which("rg")
    if not rg:
        if not install:
            raise RuntimeError("ripgrep (rg) is missing.")
        log("Fetching the checksum-verified ripgrep runtime for this platform.")
        from codex_package.ripgrep import resolve_rg_bin

        rg = str(resolve_rg_bin(TARGET_SPECS[target], None))
    exports["LEMEX_RG_BIN"] = rg
    log("Native build dependencies are ready.")
    return exports


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--env-file", required=True, type=Path)
    args = parser.parse_args()
    mode = os.environ.get("LEMEX_INSTALL_DEPS", "1")
    if mode not in {"0", "1"}:
        parser.error("LEMEX_INSTALL_DEPS must be 0 or 1")
    try:
        exports = prepare(args.target, install=mode == "1")
        args.env_file.write_text("".join(f"export {key}={shlex.quote(value)}\n" for key, value in exports.items()))
    except (RuntimeError, OSError, subprocess.CalledProcessError, tarfile.TarError) as exc:
        parser.exit(1, f"ERROR: {exc}\n")


if __name__ == "__main__":
    main()
