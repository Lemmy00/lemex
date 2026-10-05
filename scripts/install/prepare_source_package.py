#!/usr/bin/env python3
"""Complete a source install for the upstream app-server daemon contract."""

import argparse
import json
import shutil
import subprocess
from pathlib import Path


def prepare_package(package: Path, target: str, rg: Path) -> None:
    package = package.resolve()
    windows = "windows" in target
    suffix = ".exe" if windows else ""
    binaries = package / "bin"
    for name in (f"lemex{suffix}", f"lemex-code-mode-host{suffix}"):
        if not (binaries / name).is_file():
            raise RuntimeError(f"Missing source binary: {name}")
    if "linux" in target and not (package / "codex-resources/bwrap").is_file():
        raise RuntimeError("Missing Linux sandbox helper: codex-resources/bwrap")

    # Keep the public Lemex names while satisfying upstream's internal package
    # names. Relative links stay inside the package and consume no binary copy.
    for alias, original in (
        ("codex", "lemex"),
        ("codex-code-mode-host", "lemex-code-mode-host"),
    ):
        destination = binaries / f"{alias}{suffix}"
        source = binaries / f"{original}{suffix}"
        if destination.is_symlink() and destination.readlink() == Path(source.name):
            continue
        if destination.exists() or destination.is_symlink():
            raise RuntimeError(
                f"Unexpected existing compatibility entry: {destination}"
            )
        if windows:
            shutil.copy2(source, destination)
        else:
            destination.symlink_to(source.name)

    path_dir = package / "codex-path"
    path_dir.mkdir(exist_ok=True)
    rg_destination = path_dir / f"rg{suffix}"
    if rg.resolve() != rg_destination.resolve():
        shutil.copy2(rg.resolve(), rg_destination)
    (package / "codex-resources").mkdir(exist_ok=True)
    version = subprocess.check_output(
        [str(binaries / f"lemex{suffix}"), "--version"], text=True
    ).split()[1]
    metadata = {
        "layoutVersion": 1,
        "version": version,
        "target": target,
        "variant": "codex",
        "entrypoint": f"bin/codex{suffix}",
        "resourcesDir": "codex-resources",
        "pathDir": "codex-path",
    }
    manifest = package / "codex-package.json"
    temporary = manifest.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    temporary.replace(manifest)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--rg", required=True, type=Path)
    args = parser.parse_args()
    prepare_package(args.package, args.target, args.rg)


if __name__ == "__main__":
    main()
