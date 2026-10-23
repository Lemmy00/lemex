"""Exercise installed helper framing and startup failure propagation."""

import json
from pathlib import Path
import sys
import tempfile
import unittest

from smoke_voice_runtime import verify


class VoiceSmokeTests(unittest.TestCase):
    def fixture(self, root, *, fail=False):
        voice = root / "codex-resources/voice"
        (voice / "bin").mkdir(parents=True)
        (voice / "manifest.json").write_text(json.dumps({
            "buildCommit": "a" * 40, "voiceTarget": "fixture"}))
        helper = voice / "bin/codex-voice-host"
        helper.write_text(f'''#!{sys.executable}
import json, os, struct, sys
if sys.argv[1:]:
    print("a" * 40)
    sys.exit(0)
while True:
    size, = struct.unpack(">I", sys.stdin.buffer.read(4))
    request = json.loads(sys.stdin.buffer.read(size))
    kind = request["type"]
    if kind == "initializeRuntime" and {fail!r}:
        sys.exit(42)
    assert os.environ["GST_PLUGIN_SYSTEM_PATH"] == ""
    response = {{"type": {{"hello": "ready", "initializeRuntime": "runtimeReady",
                         "startTransport": "offer", "close": "closed"}}[kind]}}
    if kind == "startTransport": response["sdp"] = "m=audio fixture"
    payload = json.dumps(response).encode()
    sys.stdout.buffer.write(struct.pack(">I", len(payload)) + payload)
    sys.stdout.buffer.flush()
    if kind == "close": break
''')
        helper.chmod(0o755)

    def test_relocated_installed_helper(self):
        with tempfile.TemporaryDirectory(prefix="relocated voice ") as directory:
            root = Path(directory)
            self.fixture(root)
            verify(root)

    def test_native_initialization_failure_is_not_success(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, fail=True)
            with self.assertRaisesRegex(RuntimeError, "exited during initialization"):
                verify(root)


if __name__ == "__main__":
    unittest.main()
