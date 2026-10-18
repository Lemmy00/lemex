#!/usr/bin/env python3

"""Fail a terminal CI job unless every serialized dependency succeeded.

Parent workflows pass GitHub's `toJSON(needs)` object through the NEEDS
environment variable. Treat skipped and cancelled dependencies as failures too:
for a required fan-in job, only an explicit success is safe to accept.
"""

import json
import os


def main() -> None:
    # Keep result policy in one script so blocking-ci and postmerge-ci cannot
    # drift in how they interpret dependency conclusions.
    needs = json.loads(os.environ["NEEDS"])
    # Forks select their applicable suite explicitly. Unselected upstream jobs
    # may be skipped, but every selected job must exist and succeed.
    selected = os.environ.get("REQUIRED_JOBS")
    required = selected.split(",") if selected is not None else list(needs)
    if not required or any(not name.strip() for name in required):
        raise SystemExit("At least one required CI job must be selected")
    failures = sorted(
        (name, needs.get(name, {}).get("result", "missing"))
        for name in required
        if needs.get(name, {}).get("result") != "success"
    )

    if failures:
        print("CI dependencies did not succeed:")
        for name, result in failures:
            print(f"{name}: {result}")
        raise SystemExit(1)

    print("All CI dependencies succeeded.")


if __name__ == "__main__":
    main()
