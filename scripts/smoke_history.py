#!/usr/bin/env python3
"""Verify saved chats and continuation using an isolated local provider fixture."""

import argparse
import json
import os
import selectors
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def verify(binary: Path) -> None:
    binary = str(binary.resolve())
    with tempfile.TemporaryDirectory(prefix="lemex-history-smoke-") as directory:
        home = Path(directory)
        requests = []

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                body = json.dumps(
                    {"data": [{"id": "history-test-model", "object": "model"}]}
                ).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                request = json.loads(
                    self.rfile.read(int(self.headers.get("Content-Length", "0")))
                )
                requests.append(request)
                n = len(requests)
                answer = f"HISTORY_ASSISTANT_FIXTURE_{n}"
                events = [
                    {"type": "response.created", "response": {"id": f"resp-{n}"}},
                    {
                        "type": "response.output_item.done",
                        "item": {
                            "type": "message",
                            "role": "assistant",
                            "id": f"msg-{n}",
                            "content": [{"type": "output_text", "text": answer}],
                        },
                    },
                    {
                        "type": "response.completed",
                        "response": {
                            "id": f"resp-{n}",
                            "usage": {
                                "input_tokens": 11,
                                "input_tokens_details": {"cached_tokens": 3},
                                "output_tokens": 7,
                                "output_tokens_details": {"reasoning_tokens": 2},
                                "total_tokens": 18,
                            },
                        },
                    },
                ]
                body = "".join(
                    "event: " + e["type"] + "\ndata: " + json.dumps(e) + "\n\n"
                    for e in events
                ).encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (home / "config.toml").write_text(f"""model_provider = "history-test"
        model = "history-test-model"
        approval_policy = "never"
        sandbox_mode = "read-only"
        [model_providers.history-test]
        name = "History test fixture"
        base_url = "http://127.0.0.1:{server.server_port}/v1"
        env_key = "LEMEX_HISTORY_FIXTURE_KEY"
        wire_api = "responses"
        """)
        env = {
            k: os.environ[k]
            for k in ["PATH", "TMPDIR", "SHELL", "LANG"]
            if k in os.environ
        }
        env.update(
            HOME=str(home),
            LEMEX_HOME=str(home),
            CODEX_HOME=str(home),
            LEMEX_HISTORY_FIXTURE_KEY="local-fixture-only",
        )

        class Client:
            def __init__(self, index):
                self.errors = open(home / f"server-{index}.stderr", "wb")
                self.p = subprocess.Popen(
                    [binary, "app-server"],
                    cwd=home,
                    env=env,
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=self.errors,
                )
                self.sel = selectors.DefaultSelector()
                self.sel.register(self.p.stdout, selectors.EVENT_READ)
                self.buffer = b""
                self.messages = []
                self.counter = 0
                self.call(
                    "initialize",
                    {
                        "clientInfo": {"name": "lemex_history_fixture", "version": "1"},
                        "capabilities": {"experimentalApi": True},
                    },
                )
                self.send({"method": "initialized", "params": {}})

            def send(self, m):
                self.p.stdin.write(json.dumps(m).encode() + b"\n")
                self.p.stdin.flush()

            def next(self):
                deadline = time.monotonic() + 40
                while b"\n" not in self.buffer:
                    if time.monotonic() > deadline:
                        raise TimeoutError("waiting for local fixture response")
                    if self.sel.select(timeout=1):
                        data = os.read(self.p.stdout.fileno(), 65536)
                        if not data:
                            raise RuntimeError("local fixture server stopped")
                        self.buffer += data
                line, self.buffer = self.buffer.split(b"\n", 1)
                return json.loads(line)

            def call(self, method, params):
                self.counter += 1
                id = self.counter
                self.send({"id": id, "method": method, "params": params})
                while True:
                    m = self.next()
                    if m.get("id") == id:
                        assert "result" in m, (method, m.get("error"))
                        return m["result"]
                    self.messages.append(m)

            def complete(self):
                while True:
                    m = self.messages.pop(0) if self.messages else self.next()
                    if m.get("method") == "turn/completed":
                        assert m["params"]["turn"]["status"] == "completed", m[
                            "params"
                        ]["turn"]
                        return

            def close(self):
                self.p.stdin.close()
                try:
                    self.p.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.p.terminate()
                    self.p.wait(timeout=5)
                self.sel.close()
                self.errors.close()

        try:
            c = Client(1)
            started = c.call(
                "thread/start",
                {
                    "cwd": str(home),
                    "ephemeral": False,
                    "approvalPolicy": "never",
                    "sandbox": "read-only",
                },
            )
            id = started["thread"]["id"]
            c.call(
                "turn/start",
                {
                    "threadId": id,
                    "input": [{"type": "text", "text": "HISTORY_USER_FIXTURE_ONE"}],
                },
            )
            c.complete()
            c.close()
            c = Client(2)
            listed = c.call(
                "thread/list",
                {
                    "limit": 100,
                    "modelProviders": [],
                    "sourceKinds": ["cli", "vscode", "exec", "appServer"],
                    "currentAccountOnly": True,
                },
            )
            assert any(t["id"] == id for t in listed["data"]), (
                "fresh-process history omitted saved chat"
            )
            c.call("thread/resume", {"threadId": id})
            first = c.call("thread/turns/list", {"threadId": id, "limit": 100})
            assert len(first["data"]) == 1, first.keys()
            c.call(
                "turn/start",
                {
                    "threadId": id,
                    "input": [{"type": "text", "text": "HISTORY_USER_FIXTURE_TWO"}],
                },
            )
            c.complete()
            c.close()
            assert len(requests) == 2, len(requests)
            serialized = json.dumps(requests[1])
            assert (
                "HISTORY_USER_FIXTURE_ONE" in serialized
                and "HISTORY_ASSISTANT_FIXTURE_1" in serialized
                and "HISTORY_USER_FIXTURE_TWO" in serialized
            ), "continuation lost prior conversation context"
            c = Client(3)
            c.call("thread/resume", {"threadId": id})
            turns = c.call("thread/turns/list", {"threadId": id, "limit": 100})
            assert len(turns["data"]) == 2, "continued turn was not persisted"
            c.close()
            usage_records = [
                item["payload"]["info"]["total_token_usage"]
                for rollout in home.glob("sessions/**/*.jsonl")
                for line in rollout.read_text().splitlines()
                if (item := json.loads(line)).get("payload", {}).get("type")
                == "token_count"
                and item["payload"].get("info")
            ]
            assert usage_records, "saved coding token usage was lost"
            totals = usage_records[-1]
            assert totals["input_tokens"] == 22 and totals["cached_input_tokens"] == 6
            assert (
                totals["output_tokens"] == 14 and totals["reasoning_output_tokens"] == 4
            )
            assert totals["total_tokens"] == 36
            print(
                "PASS: saved chat listed after restart; transcript reopened; continuation retained prior context; both turns survived a second restart; coding token totals persisted."
            )
        finally:
            if "c" in locals() and c.p.poll() is None:
                c.close()
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    verify(parser.parse_args().binary)
