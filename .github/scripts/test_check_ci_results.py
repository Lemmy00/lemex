"""A gate may skip inapplicable suites, but may never hide failed required jobs."""

import json
import os
from pathlib import Path
import subprocess
import sys
import unittest


class CheckCiResultsTests(unittest.TestCase):
    def check_results(self, needs, required=None):
        env = {key: value for key, value in os.environ.items() if key != "REQUIRED_JOBS"}
        env["NEEDS"] = json.dumps({name: {"result": result} for name, result in needs.items()})
        if required is not None:
            env["REQUIRED_JOBS"] = required
        return subprocess.run(
            [sys.executable, str(Path(__file__).with_name("check_ci_results.py"))],
            env=env, text=True, capture_output=True,
        )

    def test_selected_suite_can_skip_inapplicable_upstream_jobs(self):
        result = self.check_results({"bazel": "skipped", "lemex": "success"}, "lemex")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_selected_job_must_succeed(self):
        for state in ("failure", "cancelled", "skipped"):
            with self.subTest(state=state):
                result = self.check_results({"lemex": state}, "lemex")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(state, result.stdout)

    def test_missing_selected_job_fails_closed(self):
        result = self.check_results({"lemex": "success"}, "lemxe")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing", result.stdout)

    def test_empty_selection_fails_closed(self):
        self.assertNotEqual(self.check_results({"lemex": "success"}, "").returncode, 0)
        self.assertNotEqual(self.check_results({}).returncode, 0)

    def test_default_policy_still_requires_every_dependency(self):
        self.assertNotEqual(self.check_results({"a": "success", "b": "skipped"}).returncode, 0)
        self.assertEqual(self.check_results({"a": "success", "b": "success"}).returncode, 0)


if __name__ == "__main__":
    unittest.main()
