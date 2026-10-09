#!/usr/bin/env python3
"""Offline regression coverage for portable, independent source installs."""

import importlib.util
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("import_remote_config", Path(__file__).with_name("import_remote_config.py"))
remote_config = importlib.util.module_from_spec(spec)
spec.loader.exec_module(remote_config)
package_spec = importlib.util.spec_from_file_location("prepare_source_package", Path(__file__).with_name("prepare_source_package.py"))
source_package = importlib.util.module_from_spec(package_spec)
package_spec.loader.exec_module(source_package)


class SourceInstallTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def stage(self, catalog=None):
        staged = self.root / "staged"
        staged.mkdir()
        (staged / "config.toml").write_text('model_catalog_json = "/home/alice/.lemex/models.json"\nworkspace = "/home/alice/work"\nunrelated = "/home/alice2/work"\n')
        (staged / "models.json").write_text(json.dumps(catalog if catalog is not None else {"models": [{"slug": "custom/working-model"}]}))
        return staged

    def test_remote_paths_and_catalog_are_preserved(self):
        staged = self.stage()
        destination = self.root / "custom-config"
        home = self.root / "home"
        remote_config.import_config(staged, "/home/alice/.lemex", destination, home)
        config = (destination / "config.toml").read_text()
        self.assertIn(str(destination / "models.json"), config)
        self.assertIn(str(home / "work"), config)
        self.assertIn('/home/alice2/work', config)
        self.assertEqual(json.loads((destination / "models.json").read_text())["models"][0]["slug"], "custom/working-model")

    def test_invalid_catalog_leaves_existing_settings(self):
        staged = self.stage({"models": []})
        destination = self.root / "config"
        destination.mkdir()
        for name in ("config.toml", "models.json"):
            (destination / name).write_text("original")
        with self.assertRaises(ValueError):
            remote_config.import_config(staged, "/home/alice/.lemex", destination, self.root)
        for name in ("config.toml", "models.json"):
            self.assertEqual((destination / name).read_text(), "original")

    def test_existing_settings_are_backed_up(self):
        destination = self.root / "config"
        destination.mkdir()
        (destination / "config.toml").write_text("original")
        (destination / "config.toml.bak").write_text("older backup")
        remote_config.import_config(self.stage(), "/home/alice/.lemex", destination, self.root)
        self.assertEqual((destination / "config.toml.bak").read_text(), "older backup")
        self.assertEqual((destination / "config.toml.bak.1").read_text(), "original")

    def package(self, package):
        # Local source installs leave generated runtimes in vendor/. Build an
        # isolated fixture instead of copying large binaries or old aliases.
        shutil.copytree(ROOT / "codex-cli", package, ignore=shutil.ignore_patterns("vendor"))
        node_platform = subprocess.check_output(["node", "-p", 'process.platform + ":" + process.arch'], text=True).strip()
        target = {"darwin:arm64": "aarch64-apple-darwin", "darwin:x64": "x86_64-apple-darwin", "linux:x64": "x86_64-unknown-linux-gnu", "linux:arm64": "aarch64-unknown-linux-gnu"}[node_platform]
        binary = package / "vendor" / target / "bin" / "lemex"
        binary.parent.mkdir(parents=True)
        binary.write_text('''#!/bin/sh
case "$1" in
    --version) echo "lemex 0.0.0" ;;
    --provenance)
        printf '%s\\n%s\\n' "${LEMEX_MANAGED_PACKAGE_ROOT:-}" "${CODEX_MANAGED_PACKAGE_ROOT:-}"
        ;;
    *) printf 'runtime:%s\\n' "${LEMEX_MANAGED_BY_PNPM:-npm}" ;;
esac
''')
        binary.chmod(0o755)
        helper = binary.with_name("lemex-code-mode-host")
        shutil.copy2(binary, helper)
        package_root = binary.parent.parent
        if "linux" in target:
            sandbox = package_root / "codex-resources/bwrap"
            sandbox.parent.mkdir()
            shutil.copy2(binary, sandbox)
        rg = self.root / "rg"
        rg.write_text("#!/bin/sh\nexit 0\n")
        rg.chmod(0o755)
        # Exercise upgrading the old relative symbolic-link layout as well.
        binary.with_name("codex").symlink_to("lemex")
        source_package.prepare_package(package_root, target, rg)
        return package

    @unittest.skipUnless(shutil.which("npm") and shutil.which("node"), "Node/npm required")
    def test_global_install_survives_checkout_deletion(self):
        checkout = self.package(self.root / "checkout")
        prefix = self.root / "prefix"
        # Reproduce the broken installed links to an already deleted checkout.
        modules = prefix / "lib/node_modules"
        modules.mkdir(parents=True)
        (modules / "lemex").symlink_to(self.root / "deleted-checkout")
        (prefix / "bin").mkdir()
        (prefix / "bin/lemex").symlink_to("../lib/node_modules/lemex/bin/lemex.js")
        env = dict(os.environ, npm_config_offline="true", npm_config_audit="false", npm_config_cache=str(self.root / "cache"))
        subprocess.run(["/bin/bash", "-c", 'source "$1"; CODEX_CLI_DIR="$2"; LEMEX_INSTALL_PREFIX="$3"; install_npm_package', "test", str(ROOT / "scripts/install/install_from_source.sh"), str(checkout), str(prefix)], env=env, check=True, capture_output=True, text=True)
        self.assertFalse((prefix / "lib/node_modules/lemex").is_symlink())
        shutil.rmtree(checkout)
        result = subprocess.check_output([str(prefix / "bin/lemex"), "--probe"], env=env, text=True)
        self.assertEqual(result.strip(), "runtime:npm")
        provenance = subprocess.check_output(
            [str(prefix / "bin/lemex"), "--provenance"], env=env, text=True,
        ).splitlines()
        expected_package_root = str((prefix / "lib/node_modules/lemex").resolve())
        self.assertEqual(provenance, [expected_package_root, expected_package_root])
        installed = prefix / "lib/node_modules/lemex/vendor"
        manifest = next(installed.glob("*/codex-package.json"))
        metadata = json.loads(manifest.read_text())
        package_root = manifest.parent
        self.assertTrue((package_root / metadata["entrypoint"]).is_file())
        self.assertTrue((package_root / "bin/codex-code-mode-host").is_file())
        self.assertTrue((package_root / metadata["pathDir"] / "rg").is_file())
        self.assertEqual(subprocess.check_output([str(package_root / metadata["entrypoint"]), "--version"], text=True).strip(), "lemex 0.0.0")

    @unittest.skipUnless(shutil.which("node"), "Node required")
    def test_pnpm_ownership_detected(self):
        node_modules = self.root / "node_modules"
        node_modules.mkdir()
        (node_modules / ".modules.yaml").write_text("layoutVersion: 5\n")
        package = self.package(node_modules / "lemex")
        output = subprocess.check_output(["node", str(package / "bin/lemex.js")], text=True)
        self.assertEqual(output.strip(), "runtime:1")

    def test_v8_paths_parse_on_system_bash(self):
        script = (ROOT / "scripts/install/install_from_source.sh").read_text()
        start = script.index("        local v8_artifacts=()")
        end = script.index('        export RUSTY_V8_ARCHIVE=', start)
        snippet = script[start:end]
        output = subprocess.check_output(["/bin/bash", "-c", 'parse() { local v8_paths; v8_paths=$(printf "/cache/archive\\n/cache/bindings\\n");\n' + snippet + '\nprintf "%s\\n" "${v8_artifacts[@]}"; }; parse'], text=True)
        self.assertEqual(output.splitlines(), ["/cache/archive", "/cache/bindings"])

    def run_build(self, overrides=None, *, fail=False):
        """Exercise the real build/cleanup flow with offline compiler fixtures."""
        fixture = Path(tempfile.mkdtemp(dir=self.root))
        (fixture / "codex-rs").mkdir()
        env = dict(os.environ)
        for name in (
            "CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "CARGO_INCREMENTAL",
            "CARGO_PROFILE_RELEASE_DEBUG", "CARGO_PROFILE_RELEASE_STRIP",
            "CARGO_PROFILE_RELEASE_LTO", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS",
            "LEMEX_KEEP_BUILD", "RUSTY_V8_ARCHIVE", "RUSTY_V8_SRC_BINDING_PATH",
        ):
            env.pop(name, None)
        env.update(LEMEX_BUILD="release", XDG_CACHE_HOME=str(fixture / "cache"))
        env.update(overrides or {})
        script = r'''
source "$1"
fixture="$2"
CODEX_RS_DIR="$fixture/codex-rs"
CODEX_CLI_DIR="$fixture/cli"
rustc() { printf 'host: x86_64-unknown-linux-gnu\n'; }
git() { printf 'fixture-commit\n'; }
python3() {
    if [ "$1" = "-" ]; then
        printf 'fetched\n' > "$fixture/fetch"
        printf '/fixture/archive\n/fixture/bindings\n'
    fi
}
cargo() {
    printf 'target=%s\nlto=%s\ncodegen=%s\ndebug=%s\nstrip=%s\nincremental=%s\njobs=%s\nargs=%s\n' \
        "$CARGO_TARGET_DIR" "${CARGO_PROFILE_RELEASE_LTO:-}" \
        "${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-}" "${CARGO_PROFILE_RELEASE_DEBUG:-}" \
        "${CARGO_PROFILE_RELEASE_STRIP:-}" "$CARGO_INCREMENTAL" "$CARGO_BUILD_JOBS" "$*" \
        > "$fixture/build"
    mkdir -p "$CARGO_TARGET_DIR"
    printf 'cached\n' > "$CARGO_TARGET_DIR/caller-marker"
    if [ "$3" = "--release" ]; then profile=release; else profile=dev-small; fi
    if [ "$fail_build" = 1 ]; then return 1; fi
    mkdir -p "$CARGO_TARGET_DIR/$profile"
    for binary in lemex lemex-code-mode-host bwrap; do
        printf '#!/bin/sh\necho "lemex 0.0.0"\n' > "$CARGO_TARGET_DIR/$profile/$binary"
        chmod +x "$CARGO_TARGET_DIR/$profile/$binary"
    done
}
fail_build="$3"
build_binary
'''
        result = subprocess.run(
            ["/bin/bash", "-c", script, "test",
             str(ROOT / "scripts/install/install_from_source.sh"), str(fixture),
             "1" if fail else "0"],
            env=env, capture_output=True, text=True,
        )
        observed = {}
        if (fixture / "build").exists():
            observed = dict(line.split("=", 1) for line in (fixture / "build").read_text().splitlines())
        return result, fixture, observed

    def test_release_install_removes_build_cache_and_copies_runtime(self):
        result, fixture, observed = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(observed["lto"], "off")
        self.assertEqual(observed["codegen"], "16")
        self.assertEqual(observed["debug"], "0")
        self.assertEqual(observed["strip"], "symbols")
        self.assertEqual(observed["incremental"], "0")
        self.assertIn("--locked --release", observed["args"])
        self.assertIn("--bin lemex --bin lemex-code-mode-host --bin bwrap", observed["args"])
        self.assertFalse(Path(observed["target"]).exists())
        self.assertIn("Removing temporary Cargo build cache:", result.stdout)
        self.assertTrue((fixture / "cli/vendor/x86_64-unknown-linux-gnu/bin/lemex").is_file())
        self.assertTrue((fixture / "cli/vendor/x86_64-unknown-linux-gnu/codex-resources/bwrap").is_file())

    def test_failed_build_removes_temporary_cache(self):
        result, fixture, observed = self.run_build(fail=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(Path(observed["target"]).exists())
        self.assertFalse((fixture / "cli").exists())

    def test_debug_install_uses_small_profile_and_removes_cache(self):
        result, _, observed = self.run_build({"LEMEX_BUILD": "debug"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--locked --profile dev-small", observed["args"])
        self.assertEqual(observed["lto"], "")
        self.assertEqual(observed["codegen"], "")
        self.assertFalse(Path(observed["target"]).exists())

    def test_keep_build_retains_cache_on_success_and_failure(self):
        for fail in (False, True):
            with self.subTest(fail=fail):
                result, _, observed = self.run_build({"LEMEX_KEEP_BUILD": "1"}, fail=fail)
                self.assertEqual(result.returncode == 0, not fail)
                self.assertTrue((Path(observed["target"]) / "caller-marker").is_file())
                self.assertIn("Kept Cargo build cache:", result.stdout)

    def test_caller_build_settings_and_directory_are_preserved(self):
        for target in ("reusable", str(self.root / "absolute-target")):
            with self.subTest(target=target):
                result, fixture, observed = self.run_build({
                    "CARGO_TARGET_DIR": target,
                    "CARGO_PROFILE_RELEASE_LTO": "thin",
                    "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "4",
                    "CARGO_PROFILE_RELEASE_DEBUG": "1",
                    "CARGO_PROFILE_RELEASE_STRIP": "none",
                    "CARGO_INCREMENTAL": "1",
                    "CARGO_BUILD_JOBS": "2",
                })
                self.assertEqual(result.returncode, 0, result.stderr)
                expected = Path(target) if Path(target).is_absolute() else fixture / "codex-rs" / target
                self.assertEqual(Path(observed["target"]), expected)
                self.assertTrue((expected / "caller-marker").is_file())
                self.assertEqual(observed["lto"], "thin")
                self.assertEqual(observed["codegen"], "4")
                self.assertEqual(observed["debug"], "1")
                self.assertEqual(observed["strip"], "none")
                self.assertEqual(observed["incremental"], "1")
                self.assertEqual(observed["jobs"], "2")

    def test_invalid_build_mode_does_not_fetch_or_build(self):
        result, fixture, observed = self.run_build({"LEMEX_BUILD": "invalid"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unknown LEMEX_BUILD value", result.stderr)
        self.assertEqual(observed, {})
        self.assertFalse((fixture / "fetch").exists())
        self.assertFalse((fixture / "cache").exists())


if __name__ == "__main__":
    unittest.main()
