#!/usr/bin/env python3
"""Validate a staged model catalog and relocate remote configuration paths."""

import argparse
import json
import re
import shutil
from pathlib import Path, PurePosixPath


def import_config(staged: Path, remote: str, destination: Path, home: Path) -> None:
    remote = remote.rstrip("/")
    if not remote.startswith("/") or remote == "/":
        raise ValueError("Remote config path must be an absolute directory")
    catalog_path = staged / "models.json"
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    models = catalog.get("models")
    if not isinstance(models, list) or not models:
        raise ValueError("Remote models.json must contain a nonempty models list")
    if any(not isinstance(m, dict) or not isinstance(m.get("slug"), str) or not m["slug"] for m in models):
        raise ValueError("Each remote model must have a nonempty slug")
    config_path = staged / "config.toml"
    config = config_path.read_text(encoding="utf-8")
    if not config.strip():
        raise ValueError("Remote config.toml is empty")
    # Preserve the config directory itself, including a custom LEMEX_HOME.
    # A remote user's home is a separate mapping; do not map .lemex to $HOME.
    replacements = [(remote, str(destination.resolve()))]
    remote_dir = PurePosixPath(remote)
    if remote_dir.name == ".lemex" and str(remote_dir.parent) != "/":
        replacements.append((str(remote_dir.parent), str(home.resolve())))
    pattern = "(?:" + "|".join(re.escape(old) for old, _ in replacements) + r")(?=[/\"'\s]|$)"
    mapping = dict(replacements)
    config = re.sub(pattern, lambda match: mapping[match.group()], config)
    config_path.write_text(config, encoding="utf-8")
    destination.mkdir(parents=True, exist_ok=True)
    # Keep previous settings available when explicitly importing a server setup.
    for name in ("config.toml", "models.json"):
        previous = destination / name
        if previous.exists():
            backup = destination / (name + ".bak")
            index = 1
            while backup.exists():
                backup = destination / (name + ".bak." + str(index))
                index += 1
            shutil.copy2(previous, backup)
    # All reads and validation finish before either destination is replaced.
    catalog_path.replace(destination / "models.json")
    config_path.replace(destination / "config.toml")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("staged", type=Path)
    parser.add_argument("remote")
    parser.add_argument("destination", type=Path)
    parser.add_argument("home", type=Path)
    args = parser.parse_args()
    import_config(args.staged, args.remote, args.destination, args.home)


if __name__ == "__main__":
    main()
