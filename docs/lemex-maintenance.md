# Keeping Lemex focused during upstream syncs

Lemex is an API-based coding CLI. Preserve useful upstream coding improvements while keeping the installation and interface focused.

Keep API-key setup with a configurable endpoint, RCP defaults and provider-aware model discovery, saved model/reasoning preferences, coding and sandbox tools, code mode, generic MCP tools (including Exa), local plugins and skills, voice, terminal image rendering, local logs and session token usage. Keep source installation, configuration import, and the native npm release path.

Token accounting is a core coding feature: keep input, cached-input, output and reasoning-token records, total/context usage, the status line and `/status` display, and persisted session history. Removing cloud account analytics must not remove these local records or displays.

Do not reintroduce ChatGPT onboarding, cloud tasks, account analytics dashboards, credit purchases or resets, workspace-owner notification prompts, cloud app commands, terminal pets or their CDN downloads, automatic terminal-size reporting, external analytics/feedback uploads, desktop-app promotion, or automatic Codex downloads/updates. Explicit user-configured OpenTelemetry exporters remain supported.

Shared backend protocols and authentication types may still be needed by voice, remote sessions, and configured providers. Remove unused product UI and clients at their boundaries; preserve the transport and local coding behavior that uses the shared types. Existing pet configuration keys may be read for compatibility but must not load, fetch, or animate pets.

Ship voice as a working installed feature: all four native npm packages must include the same-build helper, pinned audio runtime, notices and integrity manifest. Check initialization and transport startup from a relocated npm installation; retaining voice source alone is insufficient. Live conversations require audio devices and a compatible configured realtime provider.

Before publishing a sync, check that the removed commands stay absent and that voice, coding commands, transcript images and local token usage remain available. Run formatting, production compilation and relevant regression tests, then follow [npm release instructions](npm-release.md) from the verified merged commit.

The terminal history entry points are `lemex history` and `/history`. They must list
saved top-level chats across projects and providers, including `exec` sessions,
without exposing background subagent threads or bypassing account ownership.
Check transcript replay and continuing a saved chat after a fresh process starts;
use temporary fixtures or a private state copy rather than modifying user chats.
Keep `lemex resume` and `/resume` available for the current project's chats.

Search public interface strings, help, onboarding, diagnostics and package descriptions
for upstream product branding on every sync; use Lemex. Keep model IDs, existing
history/config compatibility readers and required third-party attribution intact.
Remove confirmed unused product integrations and dependencies as part of the sync.

## Local Rust cache cleanup

Use temporary, task-owned Cargo build and download directories for sync validation
where possible. Cleanup is required before finishing each sync, including runs
that find no upstream changes. Once local builds and tests have stopped, remove
Cargo `target` directories from all Lemex checkouts and worktrees, shared Cargo
registry/git/advisory caches, sccache data, Rustup download/temp caches, downloaded
V8 build archives, and completed temporary build/release packages.

Check active build processes and executable paths first; preserve any cache still
used by another running build. Keep installed Rust tools/toolchains, the installed
Lemex package, source code, settings, and saved chats. Retain the minimal validated
release packages and records needed for recovery until they are safely stored in
GitHub Actions or publication is verified.

Measure disk space before and after cleanup with `df`, verify removed cache paths
are absent, and smoke-check the installed `lemex` executable. Do not run Cargo
again after cleanup and recreate the caches. Report reclaimed space and any paths
that could not safely be cleared.
