import copy
import unittest

from check_npm_release_reuse import validate_source


class ReleaseReuseTests(unittest.TestCase):
    def setUp(self):
        self.source = {
            "status": "completed", "conclusion": "success",
            "event": "workflow_dispatch", "head_branch": "main",
            "path": ".github/workflows/npm-release.yml", "head_sha": "a" * 40,
            "repository": {"full_name": "Lemmy00/lemex"},
            "head_repository": {"full_name": "Lemmy00/lemex"},
        }

    def test_accepts_completed_release_from_expected_repository_and_branch(self):
        self.assertEqual(validate_source(self.source, "Lemmy00/lemex", "main"), "a" * 40)

    def test_rejects_incomplete_failed_or_unrelated_workflows(self):
        for key, value in (
            ("status", "in_progress"), ("conclusion", "failure"),
            ("conclusion", "cancelled"), ("event", "pull_request"),
            ("head_branch", "other"), ("path", ".github/workflows/other.yml"),
            ("head_sha", "main"), ("head_sha", None),
        ):
            with self.subTest(key=key, value=value):
                source = {**self.source, key: value}
                with self.assertRaises(ValueError):
                    validate_source(source, "Lemmy00/lemex", "main")

    def test_rejects_foreign_or_missing_repository_identity(self):
        for key in ("repository", "head_repository"):
            for value in ({"full_name": "someone/lemex"}, None, {}):
                with self.subTest(key=key, value=value):
                    source = copy.deepcopy(self.source)
                    source[key] = value
                    with self.assertRaises(ValueError):
                        validate_source(source, "Lemmy00/lemex", "main")


if __name__ == "__main__":
    unittest.main()
