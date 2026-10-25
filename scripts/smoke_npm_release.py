#!/usr/bin/env python3
"""Install real release tarballs through npm using an isolated local registry."""

import argparse
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import select
import subprocess
import shutil
import time

from smoke_voice_runtime import verify as verify_voice
import tempfile
import threading
from urllib.error import HTTPError
from urllib.parse import unquote
from urllib.request import urlopen

from npm_release import npm, PLATFORMS, read_manifest


def verify_ultracode(executable: Path, env: dict[str, str]) -> None:
    servers = json.loads(subprocess.check_output(
        [str(executable), "mcp", "list", "--json"], env=env, text=True, timeout=30,
    ))
    server = next((server for server in servers if server["name"] == "ultracode" and server["enabled"]), None)
    if server is None:
        raise RuntimeError("Bundled UltraCode is not enabled in the installed CLI")
    subprocess.run([str(executable.with_name("lemex-ultracode")), "--version"],
                   env=env, check=True, capture_output=True, timeout=60)
    with tempfile.TemporaryDirectory(prefix="lemex-ultracode-index-") as temporary:
        project = Path(temporary).resolve()
        (project / "example.ts").write_text(
            "export function add(a: number, b: number): number { return a + b; }\n"
            "export function double(a: number): number { return add(a, a); }\n"
        )
        config = project / "check.yaml"
        config.write_text("mcp:\n  embedding:\n    enabled: false\n  autodoc:\n    watcherEnabled: false\n")
        transport = server["transport"]
        command = [transport["command"], *transport["args"], "--no-auto-index", "--config", str(config)]
        with tempfile.TemporaryFile() as errors:
            process = subprocess.Popen(command, cwd=project,
                                       env={**env, **(transport.get("env") or {}), "MCP_EMBEDDING_ENABLED": "false"},
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors)
            buffer = b""
            request_id = 0

            def send(message):
                process.stdin.write((json.dumps({"jsonrpc": "2.0", **message}) + "\n").encode())
                process.stdin.flush()

            def request(method, params):
                nonlocal buffer, request_id
                request_id += 1
                send({"id": request_id, "method": method, "params": params})
                deadline = time.monotonic() + 90
                while True:
                    while b"\n" in buffer:
                        line, buffer = buffer.split(b"\n", 1)
                        response = json.loads(line)
                        if response.get("id") == request_id:
                            if "error" in response:
                                raise RuntimeError(f"UltraCode {method} failed: {response['error']}")
                            return response["result"]
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not select.select([process.stdout], [], [], remaining)[0]:
                        raise RuntimeError(f"UltraCode {method} timed out")
                    data = os.read(process.stdout.fileno(), 65536)
                    if not data:
                        raise RuntimeError(f"UltraCode exited during {method}")
                    buffer += data

            try:
                request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                       "clientInfo": {"name": "lemex-release-check", "version": "1"}})
                send({"method": "notifications/initialized"})
                request("tools/call", {"name": "index", "arguments": {
                    "directory": str(project), "incremental": False, "reset": False,
                    "excludePatterns": [], "fullScan": False,
                }})
                result = request("tools/call", {"name": "get_graph_stats", "arguments": {}})
                text = "\n".join(item.get("text", "") for item in result.get("content", []))
                entities = re.search(r"TOTALENTITIES:\s*(\d+)", text)
                if result.get("isError") or not entities or int(entities[1]) < 2:
                    raise RuntimeError(f"UltraCode failed to index the fixture: {text}")
                print(f"UltraCode indexed {entities[1]} entities through MCP", flush=True)
            finally:
                process.stdin.close()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
                process.stdout.close()


def smoke(launcher: Path, native: Path, *, bundled_fixtures: dict[str, Path] | None = None) -> None:
    launcher_manifest = read_manifest(launcher)
    native_manifest = read_manifest(native)
    version = launcher_manifest["version"]
    native_version = native_manifest["version"]
    payload = native.read_bytes()
    requested_payloads = []
    fixtures = bundled_fixtures or {}

    class Registry(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path.startswith("/lemex/-/"):
                requested_payloads.append(self.path)
                body = payload
                content_type = "application/octet-stream"
            elif unquote(self.path) == f"/{launcher_manifest['name']}":
                versions = {}
                for spec in PLATFORMS.values():
                    platform_version = npm.compute_platform_package_version(version, spec["npm_tag"])
                    versions[platform_version] = {
                        "name": launcher_manifest["name"], "version": platform_version,
                        "os": [spec["os"]], "cpu": [spec["cpu"]],
                        "dist": {
                            "tarball": f"http://127.0.0.1:{self.server.server_port}/lemex/-/{platform_version}.tgz",
                            "integrity": "sha512-" + base64.b64encode(hashlib.sha512(payload).digest()).decode(),
                        },
                    }
                body = json.dumps({"name": launcher_manifest["name"], "versions": versions}).encode()
                content_type = "application/json"
            elif self.path.removeprefix("/") in fixtures:
                name = self.path.removeprefix("/")
                manifest = read_manifest(fixtures[name])
                manifest["dist"] = {"tarball": f"http://127.0.0.1:{self.server.server_port}/{name}.tgz"}
                body = json.dumps({"name": name, "versions": {manifest["version"]: manifest}}).encode()
                content_type = "application/json"
            elif self.path.removeprefix("/").removesuffix(".tgz") in fixtures:
                body = fixtures[self.path.removeprefix("/").removesuffix(".tgz")].read_bytes()
                content_type = "application/octet-stream"
            else:
                # Native aliases resolve locally; third-party dependencies use
                # the actual registry, including integrity-checked tarballs.
                try:
                    with urlopen(f"https://registry.npmjs.org{self.path}", timeout=30) as response:
                        body = response.read()
                        content_type = response.headers.get("Content-Type", "application/json")
                except HTTPError as error:
                    self.send_error(error.code)
                    return
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                # npm can cancel metadata requests after selecting a platform.
                pass

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
        env = {**os.environ, "LEMEX_HOME": str(home), "HOME": str(root / "user"),
               "XDG_CONFIG_HOME": str(root / "user/config"), "XDG_DATA_HOME": str(root / "user/data"),
               "XDG_CACHE_HOME": str(root / "user/cache")}
        env.pop("LEMEX_DISABLE_ULTRACODE", None)
        output = subprocess.check_output([str(executable), "--version"], env=env, text=True, timeout=30)
        if output.strip() != f"lemex {version}":
            raise RuntimeError(f"Unexpected CLI version: {output!r}")
        for args in (["--help"], ["setup", "--help"], ["exec", "--help"]):
            subprocess.run([str(executable), *args], env=env, check=True, stdout=subprocess.DEVNULL, timeout=30)
        verify_ultracode(executable, env)
        packages = list((root / "prefix").rglob("codex-package.json"))
        if len(packages) != 1:
            raise RuntimeError(f"Expected one installed native package, found {len(packages)}")
        # Exercise the npm-installed payload again after moving it out of the
        # build and npm trees, including spaces in its new physical path.
        relocated = root / "relocated voice package"
        shutil.copytree(packages[0].parent, relocated)
        verify_voice(relocated)
        print(f"Installed and verified lemex {version} ({native_version})", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher", required=True, type=Path)
    parser.add_argument("--native", required=True, type=Path)
    args = parser.parse_args()
    smoke(args.launcher, args.native)


if __name__ == "__main__":
    main()
