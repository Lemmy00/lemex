"""Plan full builds or recover only missing, same-source native npm artifacts."""

import json
import os
import subprocess

from check_npm_release_reuse import RUNTIME_PATHS, validate_source

PLATFORMS = [
    {"runner": "ubuntu-24.04", "target": "x86_64-unknown-linux-musl", "package": "lemex-linux-x64"},
    {"runner": "ubuntu-24.04-arm", "target": "aarch64-unknown-linux-musl", "package": "lemex-linux-arm64"},
    {"runner": "macos-15-intel", "target": "x86_64-apple-darwin", "package": "lemex-darwin-x64"},
    {"runner": "macos-15", "target": "aarch64-apple-darwin", "package": "lemex-darwin-arm64"},
]


def partial_plan(source, repository, branch, jobs, artifacts):
    if source.get("conclusion") not in {"failure", "cancelled", "timed_out"}:
        raise ValueError("Recovery requires a completed unsuccessful release; reuse successful runs instead")
    revision = validate_source({**source, "conclusion": "success"}, repository, branch)
    retained = set()
    for platform in PLATFORMS:
        name = platform["package"]
        found = [a for a in artifacts if a["name"] == f"npm-{name}"]
        if not found:
            continue
        if len(found) != 1 or found[0].get("expired"):
            raise ValueError(f"Missing or ambiguous retained artifact: {name}")
        matching = [j for j in jobs if j["name"].startswith("build (") and j["name"].endswith(f", {name})")]
        if len(matching) != 1 or matching[0].get("conclusion") != "success":
            raise ValueError(f"Retained artifact has no successful platform job: {name}")
        retained.add(name)
    if not retained:
        raise ValueError("No validated platform artifacts to recover")
    missing = [p for p in PLATFORMS if p["package"] not in retained]
    if not missing:
        raise ValueError("All platforms already exist; rerun publication with the existing artifacts")
    return revision, missing


def api(path):
    return json.loads(subprocess.check_output(["gh", "api", path], text=True))


def main():
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    matrix = PLATFORMS
    run = os.environ.get("RECOVER_RUN_ID", "")
    if run:
        if os.environ.get("REUSE_RUN_ID"):
            raise ValueError("Use either recovery or full artifact reuse")
        if not run.isascii() or not run.isdigit():
            raise ValueError("Recovery run ID must contain only digits")
        repository = os.environ["REPOSITORY"]
        base = f"repos/{repository}/actions/runs/{run}"
        source = api(base)
        revision, matrix = partial_plan(source, repository, os.environ["REF_NAME"],
                                       api(base + "/jobs?per_page=100")["jobs"],
                                       api(base + "/artifacts?per_page=100")["artifacts"])
        if subprocess.run(["git", "cat-file", "-e", f"{revision}^{{commit}}"],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
            subprocess.run(["git", "fetch", "--depth=1", "origin", revision], check=True)
        subprocess.run(["git", "diff", "--exit-code", revision, "HEAD", "--", *RUNTIME_PATHS], check=True)
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"source_commit={revision}\n")
        output.write("matrix=" + json.dumps({"include": matrix}, separators=(",", ":")) + "\n")
    print(f"Build source: {revision}; platforms to build: {[p['package'] for p in matrix]}")


if __name__ == "__main__":
    main()
