"""Accept existing release artifacts only from matching, successful source builds."""

import json
import os
import re
import subprocess


RUNTIME_PATHS = [
    "codex-rs", "codex-cli", "config", "third_party", "scripts", ".github/actions",
    ".github/scripts/install-musl-build-tools.sh", ".github/scripts/macos-signing",
]


def validate_source(source: dict, repository: str, branch: str) -> str:
    expected = {
        "status": "completed", "conclusion": "success",
        "event": "workflow_dispatch", "head_branch": branch,
        "path": ".github/workflows/npm-release.yml",
    }
    if any(source.get(key) != value for key, value in expected.items()):
        raise ValueError("Source must be a successful npm release on this branch")
    for key in ("repository", "head_repository"):
        if (source.get(key) or {}).get("full_name") != repository:
            raise ValueError("Source release must belong to this repository")
    revision = source.get("head_sha", "")
    if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Invalid source revision")
    return revision


def main() -> None:
    run_id = os.environ["SOURCE_RUN_ID"]
    if not run_id.isascii() or not run_id.isdigit():
        raise SystemExit("Source run ID must contain only digits")
    repository = os.environ["REPOSITORY"]
    source = json.loads(subprocess.check_output([
        "gh", "api", f"repos/{repository}/actions/runs/{run_id}"
    ], text=True))
    revision = validate_source(source, repository, os.environ["REF_NAME"])
    if subprocess.run(
        ["git", "cat-file", "-e", f"{revision}^{{commit}}"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ).returncode:
        subprocess.run(["git", "fetch", "--depth=1", "origin", revision], check=True)
    subprocess.run([
        "git", "diff", "--exit-code", revision, "HEAD", "--", *RUNTIME_PATHS,
    ], check=True)
    print(f"Verified reusable release from {revision}")


if __name__ == "__main__":
    main()
