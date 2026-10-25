#!/usr/bin/env node
// Run the UltraCode dependency belonging to this Lemex installation.

import { createRequire } from "node:module";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
let entry;
try {
  if (Number(process.versions.node.split(".")[0]) < 24) {
    throw new Error("UltraCode requires Node.js 24 or newer.");
  }
  entry = require.resolve("ultracode");
  const typeScriptModules = path.dirname(
    path.dirname(require.resolve("typescript/package.json")),
  );
  process.env.NODE_PATH = [typeScriptModules, process.env.NODE_PATH]
    .filter(Boolean)
    .join(path.delimiter);
  // UltraCode 5.15.0 statically imports bun:ffi. Ship its runtime alongside it
  // rather than depending on a separate global Bun installation or PATH.
  if (!process.versions.bun) {
    const runtime = require.resolve("bun/bin/bun.exe");
    const child = spawn(
      runtime,
      [fileURLToPath(import.meta.url), ...process.argv.slice(2)],
      {
        stdio: "inherit",
        env: process.env,
      },
    );
    const handlers = new Map(
      ["SIGINT", "SIGTERM", "SIGHUP"].map((signal) => [
        signal,
        () => child.kill(signal),
      ]),
    );
    for (const [signal, handler] of handlers) process.on(signal, handler);
    child.on("error", (error) => {
      console.error(error.message);
      process.exitCode = 1;
    });
    child.on("exit", (code, signal) => {
      for (const [name, handler] of handlers)
        process.removeListener(name, handler);
      if (signal) process.kill(process.pid, signal);
      else process.exit(code ?? 1);
    });
    // Await the runtime without importing Bun-only modules in Node.
    await new Promise((resolve) => child.on("close", resolve));
    process.exit(process.exitCode ?? 0);
  }
} catch (error) {
  console.error(
    `${error.message}\nInstall Node.js 24+ and reinstall: npm install -g @lemmy00/lemex@latest`,
  );
  process.exit(1);
}

if (process.argv[2] === "setup") {
  entry = path.join(path.dirname(entry), "cli", "setup-command.js");
  process.argv.splice(2, 1);
} else if (
  !process.argv.some((arg) => ["--help", "-h", "--version", "-v"].includes(arg))
) {
  // Imported modules can log during initialization. Keep stdout reserved for MCP.
  process.env.MCP_QUIET_MODE = "true";
}

process.argv[1] = entry;
await import(pathToFileURL(entry).href);
