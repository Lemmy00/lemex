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


def pack_runtime(package: str, version: str, binaries: Path, output: Path) -> Path:
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
                entries = ["bin/lemex", "bin/lemex-code-mode-host", "codex-path/rg", "codex-package.json"]
                if spec["os"] == "linux":
                    entries.append("codex-resources/bwrap")
                for entry in entries:
                    member = archive.getmember(f"package/vendor/{spec['target_triple']}/{entry}")
                    if not member.isfile():
                        raise RuntimeError(f"Runtime member must be a regular file: {entry}")
                    destination = vendor / entry
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with archive.extractfile(member) as original, destination.open("wb") as copied:
                        shutil.copyfileobj(original, copied)
                    destination.chmod(member.mode & 0o777)
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
        result.append((tarball, expected_version))
    return result


def publish(directory: Path, version: str, dry_run: bool) -> None:
    packages = release_tarballs(directory, version)
    tag = "next" if "-" in version else "latest"
    for tarball, package_version in packages:
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("set-version", "pack-runtime", "repackage", "publish"):
        command = commands.add_parser(name)
        command.add_argument("--version", required=True, type=release_version)
        if name == "pack-runtime":
            command.add_argument("--package", required=True, choices=PLATFORMS)
            command.add_argument("--binaries-dir", required=True, type=Path)
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
        print(pack_runtime(args.package, args.version, args.binaries_dir, args.output_dir))
    elif args.command == "repackage":
        repackage_release(args.source_dir, args.output_dir, args.version)
    else:
        publish(args.dist_dir, args.version, args.dry_run)


if __name__ == "__main__":
    main()
