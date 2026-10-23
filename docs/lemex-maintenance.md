# Keeping Lemex focused during upstream syncs

Lemex is an API-based coding CLI. Preserve useful upstream coding improvements while keeping the installation and interface focused.

Keep API-key setup with a configurable endpoint, RCP defaults and provider-aware model discovery, saved model/reasoning preferences, coding and sandbox tools, code mode, generic MCP tools (including Exa), local plugins and skills, voice, terminal image rendering, local logs and session token usage. Keep source installation, configuration import, and the native npm release path.

Token accounting is a core coding feature: keep input, cached-input, output and reasoning-token records, total/context usage, the status line and `/status` display, and persisted session history. Removing cloud account analytics must not remove these local records or displays.

Do not reintroduce ChatGPT onboarding, cloud tasks, account analytics dashboards, credit purchases or resets, workspace-owner notification prompts, cloud app commands, terminal pets or their CDN downloads, automatic terminal-size reporting, external analytics/feedback uploads, desktop-app promotion, or automatic Codex downloads/updates. Explicit user-configured OpenTelemetry exporters remain supported.

Shared backend protocols and authentication types may still be needed by voice, remote sessions, and configured providers. Remove unused product UI and clients at their boundaries; preserve the transport and local coding behavior that uses the shared types. Existing pet configuration keys may be read for compatibility but must not load, fetch, or animate pets.

Ship voice as a working installed feature: all four native npm packages must include the same-build helper, pinned audio runtime, notices and integrity manifest. Check initialization and transport startup from a relocated npm installation; retaining voice source alone is insufficient. Live conversations require audio devices and a compatible configured realtime provider.

Before publishing a sync, check that the removed commands stay absent and that voice, coding commands, transcript images and local token usage remain available. Run formatting, production compilation and relevant regression tests, then follow [npm release instructions](npm-release.md) from the verified merged commit.
