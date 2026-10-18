#!/usr/bin/env python3
"""Offline coverage for dependency bootstrap, platform routing, and failures."""

import importlib.util
import io
import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import Mock, patch


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
os.environ.setdefault("CODEX_REPO_ROOT", str(ROOT))
spec = importlib.util.spec_from_file_location("prepare_source_deps", Path(__file__).with_name("prepare_source_deps.py"))
deps = importlib.util.module_from_spec(spec)
spec.loader.exec_module(deps)


class DependencyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="lemex dependency tests ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        self.stack.enter_context(patch.dict(os.environ, {
            "LEMEX_DEPS_DIR": str(self.root), "HOME": str(self.root),
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"), "CODEX_REPO_ROOT": str(ROOT),
        }, clear=True))

    def ready(self, **overrides):
        mocks = {}
        defaults = dict(build_tools=[], compiler_check=None, libcap_check=True,
                        library_available=True, system_packages=False)
        defaults.update(overrides)
        for name, result in defaults.items():
            mocks[name] = self.stack.enter_context(patch.object(deps, name, return_value=result))
        self.stack.enter_context(patch.object(deps.shutil, "which", return_value="/tools/rg"))
        return mocks

    def test_existing_linux_dependencies_do_not_install_anything(self):
        mocks = self.ready()
        result = deps.prepare("x86_64-unknown-linux-gnu")
        self.assertEqual(result["LEMEX_RG_BIN"], "/tools/rg")
        self.assertEqual(result["LEMEX_VENDORED_OPENSSL"], "0")
        self.assertNotIn("PKG_CONFIG_PATH", result)
        mocks["system_packages"].assert_not_called()

    def test_macos_checks_compilers_without_linux_libraries(self):
        for target in ("x86_64-apple-darwin", "aarch64-apple-darwin"):
            with self.subTest(target=target):
                mocks = self.ready()
                deps.prepare(target)
                mocks["compiler_check"].assert_called_once()
                mocks["libcap_check"].assert_not_called()
                mocks["library_available"].assert_not_called()
                mocks["system_packages"].assert_not_called()

    def test_native_windows_routes_to_wsl_before_dependency_downloads(self):
        mocks = self.ready()
        with self.assertRaisesRegex(RuntimeError, "WSL2"):
            deps.prepare("x86_64-pc-windows-msvc")
        mocks["build_tools"].assert_not_called()

    def test_unknown_os_fails_before_package_installation(self):
        mocks = self.ready()
        with self.assertRaisesRegex(RuntimeError, "Unsupported"):
            deps.prepare("x86_64-unknown-freebsd")
        mocks["system_packages"].assert_not_called()

    def test_unsupported_linux_architecture_fails_before_preparing_dependencies(self):
        mocks = self.ready()
        with self.assertRaisesRegex(RuntimeError, "x86-64 or ARM64"):
            deps.prepare("s390x-unknown-linux-gnu")
        mocks["build_tools"].assert_not_called()

    def test_missing_toolchain_fails_before_native_build(self):
        mocks = self.ready(build_tools=["cc", "make"])
        with self.assertRaisesRegex(RuntimeError, "Missing build tools: cc, make"):
            deps.prepare("x86_64-unknown-linux-gnu")
        mocks["compiler_check"].assert_not_called()

    def test_missing_xcode_sdk_has_actionable_error(self):
        mocks = self.ready()
        mocks["compiler_check"].side_effect = RuntimeError("SDK missing")
        with self.assertRaisesRegex(RuntimeError, "xcode-select --install"):
            deps.prepare("aarch64-apple-darwin")

    def test_no_sudo_uses_static_libcap_and_preserves_custom_pkg_config_path(self):
        mocks = self.ready(libcap_check=False)
        mocks["libcap_check"].side_effect = [False, False, True]
        with patch.dict(os.environ, {"PKG_CONFIG_PATH": "/custom/pkgconfig"}), patch.object(deps, "prepare_libcap") as build:
            result = deps.prepare("aarch64-unknown-linux-gnu")
        build.assert_called_once()
        self.assertEqual(result["LIBCAP_STATIC"], "1")
        self.assertEqual(result["PKG_CONFIG_PATH"], str(self.root / "aarch64-unknown-linux-gnu/libcap-2.78/lib/pkgconfig") + ":/custom/pkgconfig")

    def test_openssl_missing_uses_cargos_bundled_build(self):
        self.ready(library_available=False)
        self.assertEqual(deps.prepare("x86_64-unknown-linux-gnu")["LEMEX_VENDORED_OPENSSL"], "1")

    def test_musl_already_vendors_openssl(self):
        mocks = self.ready(library_available=False)
        self.assertEqual(deps.prepare("x86_64-unknown-linux-musl")["LEMEX_VENDORED_OPENSSL"], "0")
        mocks["library_available"].assert_not_called()

    def test_check_only_mode_does_not_download_or_install(self):
        mocks = self.ready(libcap_check=False)
        with patch.object(deps, "prepare_libcap") as build:
            with self.assertRaisesRegex(RuntimeError, "libcap headers"):
                deps.prepare("x86_64-unknown-linux-gnu", install=False)
        mocks["system_packages"].assert_not_called()
        build.assert_not_called()

    def test_missing_ripgrep_uses_verified_platform_manifest(self):
        self.ready()
        with patch.object(deps.shutil, "which", return_value=None), patch("codex_package.ripgrep.resolve_rg_bin", return_value=self.root / "rg") as fetch:
            result = deps.prepare("aarch64-apple-darwin")
        self.assertEqual(result["LEMEX_RG_BIN"], str(self.root / "rg"))
        self.assertEqual(fetch.call_args.args[0].target, "aarch64-apple-darwin")

    def test_package_manager_commands_are_noninteractive_argument_lists(self):
        for manager in deps.LINUX_PACKAGES:
            with self.subTest(manager=manager):
                args = deps.package_command(manager, ["example-devel"])
                self.assertEqual(args[0], manager)
                self.assertEqual(args[-1], "example-devel")
                self.assertTrue(any(flag in args for flag in ("-y", "--non-interactive", "--noconfirm", "--no-cache")))

    def test_no_passwordless_sudo_does_not_modify_system(self):
        with patch.object(deps.shutil, "which", side_effect=lambda name: "/usr/bin/" + name if name in ("apt-get", "sudo") else None), patch.object(deps.os, "geteuid", return_value=1000), patch.object(deps.subprocess, "run", return_value=Mock(returncode=1)) as invoke, patch.object(deps, "run") as install:
            self.assertFalse(deps.system_packages("linux"))
        invoke.assert_called_once()
        self.assertEqual(invoke.call_args.args[0], ["sudo", "-n", "true"])
        install.assert_not_called()

    def test_apt_updates_package_lists_before_installing(self):
        with patch.object(deps.shutil, "which", side_effect=lambda name: name if name == "apt-get" else None), patch.object(deps.os, "geteuid", return_value=0), patch.object(deps, "run") as invoke:
            self.assertTrue(deps.system_packages("linux"))
        self.assertEqual(invoke.call_args_list[0].args[0], ["apt-get", "update"])
        self.assertIn("libcap-dev", invoke.call_args_list[1].args[0])

    def test_libcap_bad_checksum_never_runs_source(self):
        with patch.object(deps, "urlopen", return_value=io.BytesIO(b"untrusted download")), patch.object(deps, "run") as build:
            with self.assertRaisesRegex(RuntimeError, "SHA-256"):
                deps.prepare_libcap(self.root / "prefix", {})
        build.assert_not_called()
        self.assertFalse((self.root / "prefix/lib/libcap.a").exists())

    def test_cached_libcap_is_reused_without_download(self):
        prefix = self.root / "prefix"
        for relative in ("lib/libcap.a", "include/sys/capability.h"):
            path = prefix / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("cached")
        with patch.object(deps, "urlopen") as download, patch.object(deps, "run") as build:
            deps.prepare_libcap(prefix, {})
        download.assert_not_called()
        build.assert_not_called()
        self.assertIn(str(prefix), (prefix / "lib/pkgconfig/libcap.pc").read_text())

    def test_environment_file_quotes_paths_as_data(self):
        destination = self.root / "env.sh"
        path = str(self.root / "rg '$(false)`false`")
        with patch.object(sys, "argv", ["prepare_source_deps", "--target", "aarch64-apple-darwin", "--env-file", str(destination)]), patch.object(deps, "prepare", return_value={"LEMEX_RG_BIN": path}):
            deps.main()
        value = subprocess.check_output(["/bin/bash", "-c", 'set -eu; source "$1"; printf "%s" "$LEMEX_RG_BIN"', "test", str(destination)], text=True)
        self.assertEqual(value, path)

    def test_git_bash_is_rejected_before_running_cargo(self):
        result = subprocess.run(["/bin/bash", "-c", 'source "$1"; uname() { printf "MINGW64_NT\\n"; }; check_base_deps', "test", str(ROOT / "scripts/install/install_from_source.sh")], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("WSL2", result.stderr)

    def test_writable_npm_prefix_is_preserved(self):
        prefix = self.root / "custom prefix"
        script = 'source "$1"; fixture_prefix="$2"; npm() { printf "%s\\n" "$fixture_prefix"; }; prepare_install_prefix; printf "%s" "${LEMEX_INSTALL_PREFIX:-default}"'
        result = subprocess.check_output(["/bin/bash", "-c", script, "test", str(ROOT / "scripts/install/install_from_source.sh"), str(prefix)], text=True)
        self.assertEqual(result, "default")

    @unittest.skipIf(os.geteuid() == 0, "root can write ordinary read-only directories")
    def test_unwritable_npm_prefix_uses_user_directory(self):
        prefix = self.root / "system prefix"
        prefix.mkdir()
        prefix.chmod(0o555)
        self.addCleanup(prefix.chmod, 0o755)
        script = 'source "$1"; fixture_prefix="$2"; npm() { printf "%s\\n" "$fixture_prefix"; }; prepare_install_prefix; printf "%s" "$LEMEX_INSTALL_PREFIX"'
        result = subprocess.check_output(["/bin/bash", "-c", script, "test", str(ROOT / "scripts/install/install_from_source.sh"), str(prefix)], text=True)
        self.assertEqual(result, str(self.root / ".local"))

    def test_dependency_environment_is_applied_and_temporary_file_removed(self):
        script = r'''
source "$1"
rustc() { printf 'host: x86_64-unknown-linux-gnu\n'; }
python3() {
    printf 'export LEMEX_RG_BIN="/cache/ripgrep"\nexport LIBCAP_STATIC=1\n' > "$5"
}
prepare_build_deps
test "$LEMEX_RG_BIN" = /cache/ripgrep
test "$LIBCAP_STATIC" = 1
test -z "$LEMEX_TEMP_DEPS_DIR"
'''
        subprocess.run(["/bin/bash", "-c", script, "test", str(ROOT / "scripts/install/install_from_source.sh")], check=True, env=dict(os.environ, TMPDIR=str(self.root)), capture_output=True, text=True)
        self.assertEqual(list(self.root.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
