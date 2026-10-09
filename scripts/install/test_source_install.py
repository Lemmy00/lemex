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
        shutil.copytree(ROOT / "codex-cli", package)
        node_platform = subprocess.check_output(["node", "-p", 'process.platform + ":" + process.arch'], text=True).strip()
        target = {"darwin:arm64": "aarch64-apple-darwin", "darwin:x64": "x86_64-apple-darwin", "linux:x64": "x86_64-unknown-linux-gnu", "linux:arm64": "aarch64-unknown-linux-gnu"}[node_platform]
        binary = package / "vendor" / target / "bin" / "lemex"
        binary.parent.mkdir(parents=True)
        binary.write_text('#!/bin/sh\nif [ "$1" = "--version" ]; then echo "lemex 0.0.0"; else printf "runtime:%s\\n" "${LEMEX_MANAGED_BY_PNPM:-npm}"; fi\n')
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


if __name__ == "__main__":
    unittest.main()
