"""Preserve same-build CLI binaries between parallel release and packaging jobs."""

import os
from pathlib import Path
import shutil
import sys
import tarfile

ROOT = Path(os.environ["GITHUB_WORKSPACE"]) if "GITHUB_WORKSPACE" in os.environ else Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
os.environ.setdefault("CODEX_REPO_ROOT", str(ROOT))

from codex_package.symbols import strip_binary
from codex_package.targets import TARGET_SPECS


def stage(source, output, spec):
    output.mkdir()
    for name in ("lemex", "lemex-code-mode-host"):
        destination = output / name
        shutil.copy2(source / name, destination)
        strip_binary(destination, spec)
    if spec.is_linux:
        # The app embeds this exact helper digest. Never strip it again.
        shutil.copy2(source / "bwrap", output / "bwrap")


def main():
    target = os.environ["TARGET"]
    temporary = Path(os.environ["RUNNER_TEMP"])
    output = temporary / "lemex-binaries"
    stage(Path(os.environ["CARGO_TARGET_DIR"]) / target / "release", output, TARGET_SPECS[target])
    with tarfile.open(temporary / f"{os.environ['PACKAGE']}-binaries.tar.gz", "w:gz") as archive:
        for path in sorted(output.iterdir()):
            archive.add(path, arcname=path.name)


if __name__ == "__main__":
    main()
