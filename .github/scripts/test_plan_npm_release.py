import unittest

from plan_npm_release import PLATFORMS, partial_plan


class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.source = {"status": "completed", "conclusion": "cancelled", "event": "workflow_dispatch",
                       "head_branch": "main", "path": ".github/workflows/npm-release.yml", "head_sha": "a" * 40,
                       "repository": {"full_name": "Lemmy00/lemex"}, "head_repository": {"full_name": "Lemmy00/lemex"}}
        self.jobs = [{"name": f"build (runner, target, {p['package']})", "conclusion": "success"}
                     for p in PLATFORMS if p["package"] != "lemex-darwin-x64"]
        self.artifacts = [{"name": f"npm-{p['package']}", "expired": False}
                          for p in PLATFORMS if p["package"] != "lemex-darwin-x64"]

    def plan(self):
        return partial_plan(self.source, "Lemmy00/lemex", "main", self.jobs, self.artifacts)

    def test_recovery_only_builds_missing_intel_package(self):
        revision, matrix = self.plan()
        self.assertEqual(revision, "a" * 40)
        self.assertEqual([p["package"] for p in matrix], ["lemex-darwin-x64"])

    def test_only_final_package_jobs_validate_retained_artifacts(self):
        for prefix in ("voice", "binaries"):
            self.jobs.append({"name": self.jobs[0]["name"].replace("build", prefix, 1), "conclusion": "failure"})
        self.assertEqual(self.plan()[1][0]["package"], "lemex-darwin-x64")

    def test_running_foreign_or_successful_runs_cannot_be_partial_sources(self):
        for update in ({"status": "in_progress"}, {"conclusion": "success"}, {"head_branch": "other"},
                       {"repository": {"full_name": "someone/lemex"}}):
            with self.subTest(update=update):
                original = self.source.copy()
                self.source.update(update)
                with self.assertRaises(ValueError): self.plan()
                self.source = original

    def test_failed_job_expired_or_duplicate_artifact_is_rejected(self):
        self.jobs[0]["conclusion"] = "failure"
        with self.assertRaisesRegex(ValueError, "successful platform job"): self.plan()
        self.jobs[0]["conclusion"] = "success"
        self.artifacts[0]["expired"] = True
        with self.assertRaisesRegex(ValueError, "ambiguous"): self.plan()
        self.artifacts[0]["expired"] = False
        self.artifacts.append(self.artifacts[0])
        with self.assertRaisesRegex(ValueError, "ambiguous"): self.plan()


if __name__ == "__main__": unittest.main()
