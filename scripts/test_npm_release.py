"""Packaging, registry-selection, and publication-order regression tests."""

import io
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch

import npm_release as release
from smoke_npm_release import smoke


class NpmReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def archive(self, name, manifest, files):
        archive_path = self.root / f"{name}.tgz"
        files = {**files, "package.json": json.dumps(manifest)}
        with tarfile.open(archive_path, "w:gz") as archive:
            for filename, contents in files.items():
                payload = contents.encode()
                info = tarfile.TarInfo(f"package/{filename}")
                info.size = len(payload)
                info.mode = 0o755 if "/bin/" in f"/{filename}" else 0o644
                archive.addfile(info, io.BytesIO(payload))
        return archive_path

    def release_archives(self, package_name=None):
        package_name = package_name or release.NPM_NAME
        for name, spec in release.PLATFORMS.items():
            prefix = f"vendor/{spec['target_triple']}"
            files = {f"{prefix}/{filename}": "fixture" for filename in (
                "bin/lemex", "bin/lemex-code-mode-host", "codex-path/rg", "codex-package.json"
            )}
            if spec["os"] == "linux":
                files[f"{prefix}/codex-resources/bwrap"] = "fixture"
            receipt = {"sourceCommit": "a" * 40, "target": spec["target_triple"].replace("-musl", "-gnu"),
                       "developmentOnly": False, "distribution": "publicRelease"}
            voice_files = {"bin/codex-voice-host": "voice helper", "runtime.json": json.dumps(receipt),
                           "NOTICE.md": "voice notices", "sources.json": "sources",
                           "licenses/LGPL-2.1.txt": "license", "lib/libvoice.so": "native library"}
            files.update({f"{prefix}/codex-resources/voice/{name}": value for name, value in voice_files.items()})
            digests = {"bin/lemex": hashlib.sha256(b"fixture").hexdigest(),
                       **{f"codex-resources/voice/{name}": hashlib.sha256(value.encode()).hexdigest()
                          for name, value in voice_files.items()}}
            files[f"{prefix}/codex-resources/voice/manifest.json"] = json.dumps({
                "schemaVersion": 1, "buildCommit": "a" * 40, "appVersion": "0.1.0",
                "appTarget": spec["target_triple"], "voiceTarget": receipt["target"], "sha256": digests})
            files[f"{prefix}/codex-package.json"] = json.dumps({"version": "0.1.0", "target": spec["target_triple"]})
            self.archive(name, {"name": package_name, "version": f"0.1.0-{spec['npm_tag']}"}, {
                **files, "LICENSE": "license", "NOTICE": "notice"
            })
        self.archive("lemex", {
            "name": package_name, "version": "0.1.0",
            "optionalDependencies": {
                key: f"npm:{package_name}@0.1.0-{spec['npm_tag']}" for key, spec in release.PLATFORMS.items()
            },
        }, {"bin/lemex.js": "fixture", "LICENSE": "license", "NOTICE": "notice"})

    def test_missing_or_tampered_voice_is_rejected_before_publishing(self):
        self.release_archives()
        path = self.root / "lemex-linux-x64.tgz"
        with tarfile.open(path) as archive:
            manifest = json.load(archive.extractfile("package/package.json"))
            files = {member.name.removeprefix("package/"): archive.extractfile(member).read().decode()
                     for member in archive.getmembers() if member.isfile() and member.name != "package/package.json"}
        helper = "vendor/x86_64-unknown-linux-musl/codex-resources/voice/bin/codex-voice-host"
        files[helper] = "tampered helper"
        self.archive("lemex-linux-x64", manifest, files)
        with patch.object(release.subprocess, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "digest mismatch"):
                release.publish(self.root, "0.1.0", False)
            run.assert_not_called()
        del files[helper]
        self.archive("lemex-linux-x64", manifest, files)
        with self.assertRaisesRegex(RuntimeError, "missing voice runtime files"):
            release.release_tarballs(self.root, "0.1.0")

    def test_version_updates_workspace_without_changing_registry_pins(self):
        directory = self.root / "codex-rs"
        directory.mkdir()
        (directory / "Cargo.toml").write_text('[workspace.package]\nversion = "0.0.0"\nedition = "2024"\n')
        lock = directory / "Cargo.lock"
        lock.write_text('version = 4\n\n[[package]]\nname = "local"\nversion = "0.0.0"\n\n[[package]]\nname = "remote"\nversion = "0.0.0"\nsource = "registry+example"\n')
        with patch.object(release, "ROOT", self.root):
            release.set_version("0.1.0")
        self.assertEqual(tomllib.loads((directory / "Cargo.toml").read_text())["workspace"]["package"]["version"], "0.1.0")
        packages = tomllib.loads(lock.read_text())["package"]
        self.assertEqual([package["version"] for package in packages], ["0.1.0", "0.0.0"])

    def test_staged_launcher_has_exact_native_dependencies_and_licenses(self):
        release.npm.stage_sources(self.root, "0.1.0", "lemex")
        manifest = json.loads((self.root / "package.json").read_text())
        self.assertNotIn("private", manifest)
        self.assertNotIn("scripts", manifest)
        self.assertEqual(manifest["name"], "@lemmy00/lemex")
        self.assertEqual(set(manifest["optionalDependencies"]), set(release.PLATFORMS))
        self.assertEqual(manifest["optionalDependencies"]["lemex-linux-x64"], f"npm:{release.NPM_NAME}@0.1.0-linux-x64")
        self.assertEqual(manifest["os"], ["darwin", "linux"])
        for name in ("LICENSE", "NOTICE", "README.md"):
            self.assertTrue((self.root / name).is_file(), name)

    def test_incomplete_release_cannot_start_publishing(self):
        self.release_archives()
        (self.root / "lemex-darwin-x64.tgz").unlink()
        with patch.object(release.subprocess, "run") as run:
            with self.assertRaises(FileNotFoundError):
                release.publish(self.root, "0.1.0", False)
        run.assert_not_called()

    def test_native_payload_is_required(self):
        self.release_archives()
        self.archive("lemex-linux-x64", {"name": release.NPM_NAME, "version": "0.1.0-linux-x64"}, {
            "LICENSE": "license", "NOTICE": "notice"
        })
        with self.assertRaisesRegex(RuntimeError, "Incomplete runtime"):
            release.release_tarballs(self.root, "0.1.0")

    def test_dry_run_publishes_platforms_before_latest_launcher(self):
        self.release_archives()
        with patch.object(release.subprocess, "run") as run:
            release.publish(self.root, "0.1.0", True)
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(len(commands), 5)
        self.assertTrue(all("--dry-run" in command for command in commands))
        self.assertTrue(all("--tag=release-0.1.0" in command for command in commands[:-1]))
        self.assertIn("--tag=latest", commands[-1])
        self.assertEqual(Path(commands[-1][2]).name, "lemex.tgz")

    def test_failed_platform_publish_never_publishes_launcher(self):
        self.release_archives()
        missing = subprocess.CompletedProcess([], 1, '{"error":{"code":"E404"}}', "")
        with patch.object(release.subprocess, "run", side_effect=[missing, subprocess.CalledProcessError(1, "npm publish")]) as run:
            with self.assertRaises(subprocess.CalledProcessError):
                release.publish(self.root, "0.1.0", False)
        self.assertEqual(run.call_count, 2)
        self.assertNotIn("--tag=latest", run.call_args.args[0])

    def test_pending_native_versions_prevent_launcher_publication(self):
        self.release_archives()
        def command(args, **kwargs):
            return subprocess.CompletedProcess(args, 1, '{"error":{"code":"E404"}}', "") if args[1] == "view" else subprocess.CompletedProcess(args, 0)
        with patch.object(release.subprocess, "run", side_effect=command) as run, patch.object(
                release, "wait_for_publication", side_effect=RuntimeError("scan pending")):
            with self.assertRaisesRegex(RuntimeError, "scan pending"):
                release.publish(self.root, "0.1.0", False)
        publishes = [c.args[0] for c in run.call_args_list if c.args[0][1] == "publish"]
        self.assertEqual(len(publishes), 4)
        self.assertTrue(all("--tag=latest" not in command for command in publishes))

    def test_registry_scan_delay_is_polled_until_exact_artifact_is_available(self):
        path = self.archive("scan", {"name": release.NPM_NAME, "version": "0.1.0"}, {})
        integrity = "sha512-" + release.base64.b64encode(hashlib.sha512(path.read_bytes()).digest()).decode()
        metadata = {"name": release.NPM_NAME, "version": "0.1.0", "dist": {"integrity": integrity}}
        unavailable = release.HTTPError("registry", 404, "scan pending", {}, None)
        with patch.object(release, "urlopen", side_effect=[unavailable, io.BytesIO(json.dumps(metadata).encode())]), patch.object(release.time, "sleep") as sleep:
            release.wait_for_publication([(path, "0.1.0")])
        sleep.assert_called_once()

    def test_registry_integrity_mismatch_is_never_treated_as_scan_delay(self):
        path = self.archive("scan", {"name": release.NPM_NAME, "version": "0.1.0"}, {})
        metadata = {"name": release.NPM_NAME, "version": "0.1.0", "dist": {"integrity": "different"}}
        with patch.object(release, "urlopen", return_value=io.BytesIO(json.dumps(metadata).encode())), patch.object(release.time, "sleep") as sleep:
            with self.assertRaisesRegex(RuntimeError, "differs from validated artifact"):
                release.wait_for_publication([(path, "0.1.0")])
        sleep.assert_not_called()

    def test_registry_timeout_retains_pending_version_for_recovery(self):
        path = self.archive("scan", {"name": release.NPM_NAME, "version": "0.1.0"}, {})
        with patch.object(release, "urlopen", side_effect=release.HTTPError("registry", 404, "pending", {}, None)):
            with self.assertRaisesRegex(RuntimeError, "retain this version and its artifacts"):
                release.wait_for_publication([(path, "0.1.0")], timeout=0)

    def test_existing_version_must_match_exact_tarball(self):
        self.release_archives()
        existing = subprocess.CompletedProcess([], 0, '"sha512-different"', "")
        with patch.object(release.subprocess, "run", return_value=existing) as run:
            with self.assertRaisesRegex(RuntimeError, "different bytes"):
                release.publish(self.root, "0.1.0", False)
        self.assertEqual(run.call_count, 1)

    @unittest.skipUnless(shutil.which("npm"), "npm required")
    def test_repackage_legacy_artifacts_preserves_every_native_file(self):
        self.release_archives("lemex")
        output = self.root / "scoped"
        release.repackage_release(self.root, output, "0.1.0")
        release.release_tarballs(output, "0.1.0")
        for name in release.PLATFORMS:
            with tarfile.open(self.root / f"{name}.tgz") as original, tarfile.open(output / f"{name}.tgz") as repackaged:
                self.assertEqual(json.load(repackaged.extractfile("package/package.json"))["name"], "@lemmy00/lemex")
                for member in original.getmembers():
                    if member.isfile() and member.name.startswith("package/vendor/"):
                        self.assertEqual(original.extractfile(member).read(), repackaged.extractfile(member.name).read())

    def test_repackage_rejects_linked_runtime_members(self):
        self.release_archives("lemex")
        path = self.root / "lemex-linux-x64.tgz"
        with tarfile.open(path, "w:gz") as archive:
            manifest = json.dumps({"name": "lemex", "version": "0.1.0-linux-x64"}).encode()
            member = tarfile.TarInfo("package/package.json")
            member.size = len(manifest)
            archive.addfile(member, io.BytesIO(manifest))
            link = tarfile.TarInfo("package/vendor/x86_64-unknown-linux-musl/bin/lemex")
            link.type = tarfile.SYMTYPE
            link.linkname = "/unexpected"
            archive.addfile(link)
        with self.assertRaisesRegex(RuntimeError, "regular file"):
            release.repackage_release(self.root, self.root / "scoped", "0.1.0")

    @unittest.skipUnless(shutil.which("npm"), "npm required")
    def test_linux_runtime_preserves_sandbox_bytes_and_native_version(self):
        binaries = self.root / "binaries"
        binaries.mkdir()
        for name, contents in {
            "lemex": '#!/bin/sh\necho "lemex 0.1.0"\n',
            "lemex-code-mode-host": "host fixture",
            "bwrap": "sandbox bytes covered by the embedded digest",
            "rg": "ripgrep fixture",
        }.items():
            (binaries / name).write_text(contents)
            (binaries / name).chmod(0o755)
        def assemble(package, helper, target, commit, output, **kwargs):
            shutil.copytree(package, output)
        options = dict(voice_helper=binaries / "voice", voice_runtime=binaries, build_commit="a" * 40)
        with patch.object(release.voice, "assemble", side_effect=assemble) as voice, patch.object(release, "strip_binary") as strip, patch.object(release, "resolve_rg_bin", return_value=binaries / "rg"):
            result = release.pack_runtime("lemex-linux-x64", "0.1.0", binaries, self.root, **options)
            self.assertEqual(strip.call_count, 2)
            self.assertEqual(voice.call_args.args[2:4], ("x86_64-unknown-linux-gnu", "a" * 40))
            with tarfile.open(result, "r:gz") as archive:
                prefix = "package/vendor/x86_64-unknown-linux-musl"
                self.assertEqual(archive.extractfile(f"{prefix}/codex-resources/bwrap").read(), (binaries / "bwrap").read_bytes())
                metadata = json.load(archive.extractfile(f"{prefix}/codex-package.json"))
                self.assertEqual(metadata["version"], "0.1.0")
            with self.assertRaisesRegex(RuntimeError, "does not match npm version"):
                release.pack_runtime("lemex-linux-x64", "0.2.0", binaries, self.root, **options)

    @unittest.skipUnless(shutil.which("node") and shutil.which("npm"), "Node.js and npm required")
    def test_real_npm_install_resolves_only_current_platform_alias(self):
        platform = subprocess.check_output(["node", "-p", "process.platform + '-' + process.arch"], text=True).strip()
        name = f"lemex-{platform}"
        if name not in release.PLATFORMS:
            self.skipTest("Unsupported host")
        spec = release.PLATFORMS[name]
        staging = self.root / "launcher"
        staging.mkdir()
        release.npm.stage_sources(staging, "0.1.0", "lemex")
        launcher = release.npm.run_npm_pack(staging, self.root / "lemex.tgz")
        # A tiny executable makes this a portable npm/launcher integration test.
        # CI repeats the same install test with the actual compiled CLI.
        native = self.archive(name, {
            "name": release.NPM_NAME, "version": f"0.1.0-{spec['npm_tag']}",
            "os": [spec["os"]], "cpu": [spec["cpu"]],
        }, {f"vendor/{spec['target_triple']}/codex-package.json": "{}", f"vendor/{spec['target_triple']}/bin/lemex": '#!/bin/sh\nif [ "$1" = "--version" ]; then echo "lemex 0.1.0"; fi\n'})
        with patch("smoke_npm_release.verify_voice") as verify:
            smoke(launcher, native)
            verify.assert_called_once()


if __name__ == "__main__":
    unittest.main()
