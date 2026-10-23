from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from stage_npm_binaries import stage, TARGET_SPECS


class StageTests(unittest.TestCase):
    def test_linux_sandbox_digest_bytes_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            for name in ("lemex", "lemex-code-mode-host", "bwrap"):
                (source / name).write_bytes(name.encode())
            with patch("stage_npm_binaries.strip_binary") as strip:
                stage(source, root / "output", TARGET_SPECS["x86_64-unknown-linux-musl"])
            self.assertEqual([call.args[0].name for call in strip.call_args_list], ["lemex", "lemex-code-mode-host"])
            self.assertEqual((root / "output/bwrap").read_bytes(), (source / "bwrap").read_bytes())


if __name__ == "__main__": unittest.main()
