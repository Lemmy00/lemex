#!/usr/bin/env python3
"""Install real release tarballs through npm using an isolated local registry."""

import argparse
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading

from npm_release import npm, PLATFORMS, read_manifest


def smoke(launcher: Path, native: Path) -> None:
    launcher_manifest = read_manifest(launcher)
    native_manifest = read_manifest(native)
    version = launcher_manifest["version"]
    native_version = native_manifest["version"]
    payload = native.read_bytes()
    requested_payloads = []

    class Registry(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path.startswith("/lemex/-/"):
                requested_payloads.append(self.path)
                body = payload
                content_type = "application/octet-stream"
            else:
                versions = {}
                for spec in PLATFORMS.values():
                    platform_version = npm.compute_platform_package_version(version, spec["npm_tag"])
                    versions[platform_version] = {
                        "name": "lemex", "version": platform_version,
                        "os": [spec["os"]], "cpu": [spec["cpu"]],
                        "dist": {
                            "tarball": f"http://127.0.0.1:{self.server.server_port}/lemex/-/{platform_version}.tgz",
                            "integrity": "sha512-" + base64.b64encode(hashlib.sha512(payload).digest()).decode(),
                        },
                    }
                body = json.dumps({"name": "lemex", "versions": versions}).encode()
                content_type = "application/json"
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *args):
            pass

    with tempfile.TemporaryDirectory(prefix="lemex-npm-smoke-") as temporary:
        root = Path(temporary)
        server = ThreadingHTTPServer(("127.0.0.1", 0), Registry)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            # This must resolve optional aliases and OS/CPU filters exactly as a
            # registry install would, without touching the user's global install.
            subprocess.run([
                "npm", "install", "-g", "--prefix", str(root / "prefix"),
                "--cache", str(root / "cache"), "--no-audit", "--no-fund",
                f"--registry=http://127.0.0.1:{server.server_port}", str(launcher.resolve()),
            ], cwd=root, check=True, timeout=180)
        finally:
            server.shutdown()
            server.server_close()
            worker.join()
        if requested_payloads != [f"/lemex/-/{native_version}.tgz"]:
            raise RuntimeError(f"npm selected unexpected native payloads: {requested_payloads}")
        executable = root / "prefix/bin/lemex"
        home = root / "home"
        home.mkdir()
        env = {**os.environ, "LEMEX_HOME": str(home)}
        output = subprocess.check_output([str(executable), "--version"], env=env, text=True, timeout=30)
        if output.strip() != f"lemex {version}":
            raise RuntimeError(f"Unexpected CLI version: {output!r}")
        for args in (["--help"], ["setup", "--help"], ["exec", "--help"]):
            subprocess.run([str(executable), *args], env=env, check=True, stdout=subprocess.DEVNULL, timeout=30)
        print(f"Installed and verified lemex {version} ({native_version})", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher", required=True, type=Path)
    parser.add_argument("--native", required=True, type=Path)
    args = parser.parse_args()
    smoke(args.launcher, args.native)


if __name__ == "__main__":
    main()
