#!/usr/bin/env python3
"""Build and publish the precompiled Lemex npm release packages."""

import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen
from pathlib import PurePosixPath
import tomllib

ROOT = Path(__file__).resolve().parent.parent
os.environ.setdefault("CODEX_REPO_ROOT", str(ROOT))

from codex_package.cli import parse_package_version
from codex_package.ripgrep import resolve_rg_bin
from codex_package.symbols import strip_binary
from codex_package.targets import TARGET_SPECS


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


npm = load_module("build_npm_package", ROOT / "codex-cli/scripts/build_npm_package.py")
voice = load_module("assemble_voice_package", ROOT / "third_party/voice/assemble_package.py")
runtime = load_module("prepare_source_package", ROOT / "scripts/install/prepare_source_package.py")
PLATFORMS = {
    name: npm.LEMEX_PLATFORM_PACKAGES[name]
    for name in npm.PACKAGE_EXPANSIONS["lemex"]
    if name != "lemex"
}
REGISTRY = "https://registry.npmjs.org/"
NPM_NAME = npm.LEMEX_NPM_NAME


def release_version(value: str) -> str:
    version = parse_package_version(value)
    if "+" in version:
        raise argparse.ArgumentTypeError("npm releases cannot use build metadata")
    return version


def set_version(version: str) -> None:
    """Version workspace members together without updating third-party pins."""
    manifest = ROOT / "codex-rs/Cargo.toml"
    source = manifest.read_text()
    old_version = tomllib.loads(source)["workspace"]["package"]["version"]
    source, count = re.subn(
        r'(\[workspace\.package\]\s*\n(?:[^\[]*?\n)?)version = "[^"]+"',
        lambda match: match[0].rsplit('"', 2)[0] + f'"{version}"',
        source,
        count=1,
    )
    if count != 1:
        raise RuntimeError("Could not locate workspace.package.version")
    lockfile = ROOT / "codex-rs/Cargo.lock"
    sections = lockfile.read_text().split("[[package]]")
    for index, section in enumerate(sections[1:], 1):
        fields = tomllib.loads(section)
        if "source" not in fields and fields["version"] == old_version:
            sections[index] = section.replace(
                f'version = "{old_version}"', f'version = "{version}"', 1
            )
    manifest.write_text(source)
    lockfile.write_text("[[package]]".join(sections))


def pack_runtime(package: str, version: str, binaries: Path, output: Path, *,
                 voice_helper: Path, voice_runtime: Path, build_commit: str) -> Path:
    spec = TARGET_SPECS[PLATFORMS[package]["target_triple"]]
    with tempfile.TemporaryDirectory(prefix="lemex-release-") as temporary:
        staging = Path(temporary)
        vendor = staging / "vendor" / spec.target
        (vendor / "bin").mkdir(parents=True)
        (vendor / "codex-resources").mkdir()
        for name in ("lemex", "lemex-code-mode-host"):
            destination = vendor / "bin" / name
            shutil.copy2(binaries / name, destination)
            strip_binary(destination, spec)
        if spec.is_linux:
            # The CLI embeds this helper's digest. Preserve the exact build bytes.
            shutil.copy2(binaries / "bwrap", vendor / "codex-resources/bwrap")
        runtime.prepare_package(vendor, spec.target, resolve_rg_bin(spec, None))
        metadata = json.loads((vendor / "codex-package.json").read_text())
        if metadata["version"] != version:
            raise RuntimeError(
                f"Native version {metadata['version']} does not match npm version {version}"
            )
        voiced = staging / "voiced"
        voice.assemble(vendor, voice_helper, spec.target.replace("-musl", "-gnu"),
                       build_commit, voiced, runtime=voice_runtime, release_version=version)
        shutil.rmtree(vendor)
        voiced.rename(vendor)
        package_dir = staging / "package"
        package_dir.mkdir()
        npm.stage_sources(package_dir, version, package)
        npm.copy_native_binaries(
            staging / "vendor", package_dir, [npm.LEMEX_PACKAGE_COMPONENT], {spec.target}
        )
        return npm.run_npm_pack(package_dir, output / f"{package}.tgz")


def read_manifest(tarball: Path) -> dict:
    with tarfile.open(tarball, "r:gz") as archive:
        return json.load(archive.extractfile("package/package.json"))


def repackage_release(source: Path, output: Path, version: str) -> None:
    """Restage package metadata and the launcher while preserving native bytes."""
    if source.resolve() == output.resolve():
        raise RuntimeError("Use a separate output directory for repackaged artifacts")
    for name, spec in PLATFORMS.items():
        with tempfile.TemporaryDirectory(prefix="lemex-repackage-") as temporary:
            staging = Path(temporary)
            vendor = staging / "vendor" / spec["target_triple"]
            with tarfile.open(source / f"{name}.tgz", "r:gz") as archive:
                manifest = json.load(archive.extractfile("package/package.json"))
                if manifest.get("name") not in {"lemex", NPM_NAME} or manifest.get("version") != f"{version}-{spec['npm_tag']}":
                    raise RuntimeError(f"Unexpected source package: {name}")
                prefix = f"package/vendor/{spec['target_triple']}/"
                for member in archive.getmembers():
                    if not member.name.startswith(prefix) or member.isdir():
                        continue
                    entry = member.name[len(prefix):]
                    if not member.isfile():
                        raise RuntimeError(f"Runtime member must be a regular file: {entry}")
                    if not entry or PurePosixPath(entry).is_absolute() or ".." in PurePosixPath(entry).parts:
                        raise RuntimeError("Unsafe runtime member")
                    destination = vendor / entry
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with archive.extractfile(member) as original, destination.open("wb") as copied:
                        shutil.copyfileobj(original, copied)
                    destination.chmod(member.mode & 0o777)
                validate_voice_archive(archive, prefix.rstrip("/"), version, spec["target_triple"])
            metadata = json.loads((vendor / "codex-package.json").read_text())
            if metadata.get("version") != version or metadata.get("target") != spec["target_triple"]:
                raise RuntimeError(f"Unexpected native runtime metadata: {name}")
            package_dir = staging / "package"
            package_dir.mkdir()
            npm.stage_sources(package_dir, version, name)
            npm.copy_native_binaries(staging / "vendor", package_dir, [npm.LEMEX_PACKAGE_COMPONENT], {spec["target_triple"]})
            npm.run_npm_pack(package_dir, output / f"{name}.tgz")
    with tempfile.TemporaryDirectory(prefix="lemex-launcher-") as temporary:
        package_dir = Path(temporary)
        npm.stage_sources(package_dir, version, "lemex")
        npm.run_npm_pack(package_dir, output / "lemex.tgz")
    release_tarballs(output, version)


def validate_voice_archive(archive: tarfile.TarFile, prefix: str, version: str, target: str) -> None:
    """Reject missing, mixed-build, tampered or unlisted native voice payloads."""
    voice_prefix = f"{prefix}/codex-resources/voice"
    try:
        manifest = json.load(archive.extractfile(f"{voice_prefix}/manifest.json"))
        receipt = json.load(archive.extractfile(f"{voice_prefix}/runtime.json"))
    except (KeyError, TypeError, ValueError) as error:
        raise RuntimeError("Missing or invalid voice runtime manifest") from error
    commit = manifest.get("buildCommit", "")
    if (not re.fullmatch(r"[0-9a-f]{40}", commit)
            or manifest.get("schemaVersion") != 1
            or manifest.get("appTarget") != target
            or manifest.get("voiceTarget") != target.replace("-musl", "-gnu")
            or manifest.get("appVersion") != version
            or receipt.get("sourceCommit") != commit
            or receipt.get("target") != manifest["voiceTarget"]
            or receipt.get("developmentOnly") is not False
            or receipt.get("distribution") != "publicRelease"):
        raise RuntimeError("Voice runtime provenance does not match the release")
    digests = manifest.get("sha256", {})
    required = {"bin/lemex", "codex-resources/voice/bin/codex-voice-host",
                "codex-resources/voice/runtime.json", "codex-resources/voice/NOTICE.md",
                "codex-resources/voice/sources.json", "codex-resources/voice/licenses/LGPL-2.1.txt"}
    if not required.issubset(digests):
        raise RuntimeError("Incomplete voice runtime manifest")
    listed = {f"{prefix}/{name}" for name in digests}
    actual = {member.name for member in archive.getmembers()
              if member.name.startswith(voice_prefix + "/") and not member.isdir()}
    if actual != {name for name in listed if name.startswith(voice_prefix + "/")} | {voice_prefix + "/manifest.json"}:
        raise RuntimeError("Unlisted or missing voice runtime files")
    for name, expected in digests.items():
        if PurePosixPath(name).is_absolute() or ".." in PurePosixPath(name).parts:
            raise RuntimeError("Unsafe voice runtime path")
        try:
            member = archive.getmember(f"{prefix}/{name}")
        except KeyError as error:
            raise RuntimeError(f"Missing voice runtime member: {name}") from error
        if not member.isfile():
            raise RuntimeError(f"Voice runtime member must be a regular file: {name}")
        with archive.extractfile(member) as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != expected:
                raise RuntimeError(f"Voice runtime digest mismatch: {name}")


def release_tarballs(directory: Path, version: str) -> list[tuple[Path, str]]:
    """Require every platform before allowing any registry mutation."""
    packages = [(name, f"{version}-{spec['npm_tag']}") for name, spec in PLATFORMS.items()]
    packages.append(("lemex", version))
    result = []
    for name, expected_version in packages:
        tarball = directory / f"{name}.tgz"
        manifest = read_manifest(tarball)
        if manifest["name"] != NPM_NAME or manifest["version"] != expected_version:
            raise RuntimeError(f"Unexpected package identity in {tarball}")
        with tarfile.open(tarball, "r:gz") as archive:
            files = set(archive.getnames())
            if not {"package/LICENSE", "package/NOTICE"}.issubset(files):
                raise RuntimeError(f"Missing license or notices in {tarball}")
            if name == "lemex":
                expected_dependencies = {
                    key: f"npm:{NPM_NAME}@{version}-{spec['npm_tag']}"
                    for key, spec in PLATFORMS.items()
                }
                if manifest.get("optionalDependencies") != expected_dependencies:
                    raise RuntimeError("Launcher dependencies do not match this release")
                if "package/bin/lemex.js" not in files:
                    raise RuntimeError("Missing npm launcher")
            else:
                spec = PLATFORMS[name]
                prefix = f"package/vendor/{spec['target_triple']}"
                required = {f"{prefix}/{entry}" for entry in (
                    "bin/lemex", "bin/lemex-code-mode-host", "codex-package.json", "codex-path/rg"
                )}
                if spec["os"] == "linux":
                    required.add(f"{prefix}/codex-resources/bwrap")
                if not required.issubset(files):
                    raise RuntimeError(f"Incomplete runtime in {tarball}: {required - files}")
                validate_voice_archive(archive, prefix, version, spec["target_triple"])
        result.append((tarball, expected_version))
    return result


def wait_for_publication(packages: list[tuple[Path, str]], *, timeout: float = 1800,
                         interval: float = 30) -> None:
    """Wait for npm's asynchronous scans without republishing accepted versions."""
    pending = {}
    for path, version in packages:
        with path.open("rb") as source:
            pending[version] = "sha512-" + base64.b64encode(hashlib.file_digest(source, "sha512").digest()).decode()
    deadline = time.monotonic() + timeout
    while pending:
        for version, integrity in list(pending.items()):
            url = (f"{REGISTRY}{quote(NPM_NAME, safe='@')}/{quote(version, safe='')}"
                   f"?lemex-release-check={time.time_ns()}")
            try:
                request = Request(url, headers={"Accept": "application/json", "Cache-Control": "no-cache"})
                with urlopen(request, timeout=30) as response:
                    metadata = json.load(response)
            except HTTPError as error:
                if error.code in {404, 408, 429} or error.code >= 500:
                    continue
                raise
            except (URLError, TimeoutError):
                continue
            if (metadata.get("name") != NPM_NAME or metadata.get("version") != version
                    or metadata.get("dist", {}).get("integrity") != integrity):
                raise RuntimeError(f"Registry package differs from validated artifact: {NPM_NAME}@{version}")
            del pending[version]
            print(f"Verified public registry availability and integrity: {NPM_NAME}@{version}", flush=True)
        if pending:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise RuntimeError(
                    f"Registry availability timed out for {', '.join(pending)}. "
                    "npm scanning or manual review may still be pending; retain this version and its artifacts."
                )
            print(f"Waiting for npm registry availability: {', '.join(pending)}", flush=True)
            time.sleep(min(interval, remaining))


def publish(directory: Path, version: str, dry_run: bool) -> None:
    packages = release_tarballs(directory, version)
    tag = "next" if "-" in version else "latest"
    for tarball, package_version in packages:
        if package_version == version and not dry_run:
            # Successful upload does not imply an installable dependency: npm
            # scans each version before exposing it to public registry clients.
            wait_for_publication(packages[:-1])
        # Publish native variants under a temporary tag, then publish the launcher
        # last. A failed native publish must never move the public latest tag.
        package_tag = tag if package_version == version else f"release-{version}"
        if not dry_run:
            existing = subprocess.run(
                ["npm", "view", f"{NPM_NAME}@{package_version}", "dist.integrity", "--json", f"--registry={REGISTRY}"],
                capture_output=True, text=True,
            )
            if existing.returncode == 0:
                integrity = "sha512-" + base64.b64encode(hashlib.sha512(tarball.read_bytes()).digest()).decode()
                if json.loads(existing.stdout) != integrity:
                    raise RuntimeError(f"{NPM_NAME}@{package_version} already exists with different bytes")
                print(f"Already published {NPM_NAME}@{package_version}; verified integrity", flush=True)
                continue
            error = json.loads(existing.stdout or "{}")
            if error.get("error", {}).get("code") != "E404":
                raise RuntimeError(f"Could not check registry state: {existing.stderr}")
        command = ["npm", "publish", str(tarball.resolve()), "--access=public", f"--tag={package_tag}", f"--registry={REGISTRY}"]
        if dry_run:
            command.append("--dry-run")
        subprocess.run(command, check=True)
    if not dry_run:
        wait_for_publication(packages[-1:])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("set-version", "pack-runtime", "repackage", "publish"):
        command = commands.add_parser(name)
        command.add_argument("--version", required=True, type=release_version)
        if name == "pack-runtime":
            command.add_argument("--package", required=True, choices=PLATFORMS)
            command.add_argument("--binaries-dir", required=True, type=Path)
            command.add_argument("--voice-helper", required=True, type=Path)
            command.add_argument("--voice-runtime", required=True, type=Path)
            command.add_argument("--build-commit", required=True)
            command.add_argument("--output-dir", required=True, type=Path)
        if name == "publish":
            command.add_argument("--dist-dir", required=True, type=Path)
            command.add_argument("--dry-run", action="store_true")
        if name == "repackage":
            command.add_argument("--source-dir", required=True, type=Path)
            command.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    if args.command == "set-version":
        set_version(args.version)
    elif args.command == "pack-runtime":
        print(pack_runtime(args.package, args.version, args.binaries_dir, args.output_dir,
                           voice_helper=args.voice_helper, voice_runtime=args.voice_runtime,
                           build_commit=args.build_commit))
    elif args.command == "repackage":
        repackage_release(args.source_dir, args.output_dir, args.version)
    else:
        publish(args.dist_dir, args.version, args.dry_run)


if __name__ == "__main__":
    main()
