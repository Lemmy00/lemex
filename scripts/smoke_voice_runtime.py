#!/usr/bin/env python3
"""Verify an installed, relocatable voice payload without opening audio devices."""

import argparse
import json
import os
from pathlib import Path
import select
import struct
import subprocess
import time


def verify(package: Path) -> None:
    package = package.resolve(strict=True)
    voice = package / "codex-resources/voice"
    manifest = json.loads((voice / "manifest.json").read_text())
    helper = voice / "bin/codex-voice-host"
    commit = manifest["buildCommit"]
    actual = subprocess.check_output([str(helper), "--build-commit"], text=True, timeout=30).strip()
    if actual != commit:
        raise RuntimeError("Installed voice helper has the wrong build commit")
    # Match the application's fixed runtime environment. No system GStreamer
    # installation, plugin search path, registry cache, or loader override is used.
    env = {key: value for key, value in os.environ.items()
           if key in {"HOME", "TMPDIR", "TEMP", "TMP", "XDG_RUNTIME_DIR"}}
    env.update({"GST_PLUGIN_PATH": "", "GST_PLUGIN_PATH_1_0": "",
                "GST_PLUGIN_SYSTEM_PATH": "", "GST_PLUGIN_SYSTEM_PATH_1_0": "",
                "GST_REGISTRY": "/dev/null", "GST_REGISTRY_UPDATE": "no",
                "GST_REGISTRY_FORK": "no"})
    process = subprocess.Popen([str(helper)], cwd=package, env=env,
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.DEVNULL)

    def read_exact(size: int) -> bytes:
        result = bytearray()
        deadline = time.monotonic() + 35
        while len(result) < size:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([process.stdout], [], [], remaining)[0]:
                raise RuntimeError("Voice helper response timed out")
            chunk = os.read(process.stdout.fileno(), size - len(result))
            if not chunk:
                raise RuntimeError(f"Voice helper exited during initialization: {process.poll()}")
            result.extend(chunk)
        return bytes(result)

    def exchange(request: dict, expected: str) -> dict:
        payload = json.dumps(request).encode()
        process.stdin.write(struct.pack(">I", len(payload)) + payload)
        process.stdin.flush()
        size, = struct.unpack(">I", read_exact(4))
        if size > 128 * 1024:
            raise RuntimeError("Oversized voice helper response")
        response = json.loads(read_exact(size))
        if response.get("type") != expected:
            raise RuntimeError(f"Unexpected voice response type (expected {expected})")
        return response

    try:
        exchange({"type": "hello", "protocol": 1, "buildCommit": commit}, "ready")
        exchange({"type": "initializeRuntime"}, "runtimeReady")
        # This checks the real WebRTC/Opus transport without a backend request,
        # retained audio, or CI microphone access. Never print SDP credentials.
        offer = exchange({"type": "startTransport"}, "offer")
        if "m=audio" not in offer.get("sdp", ""):
            raise RuntimeError("Voice transport did not negotiate audio")
        exchange({"type": "close"}, "closed")
        if process.wait(timeout=10) != 0:
            raise RuntimeError("Voice helper failed to shut down cleanly")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
        process.stdin.close()
        process.stdout.close()
    print(f"Verified installed voice runtime and audio offer ({manifest['voiceTarget']}, {commit})", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    verify(parser.parse_args().package)
