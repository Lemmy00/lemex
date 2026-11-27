# November 27 upstream refresh

This refresh starts from Lemex `9312e11005fae4409ee36b0bb6458f6dc109a017` and
incorporates 900 upstream commits after
`e58bf8f5233b51e7cd1679d947e3028c1ed91b06`, through
`afe555b9d3e95b4f58b247212565070f1555bc5d`. Both source histories are retained.

Upstream changes include the GPT-6.2 catalog, saved side chats, improvements to
partially loaded history and prompt editing, inline Markdown images, the diff
sidebar, MCP startup behavior, and approval-review provenance and timing.

Lemex retains its own command, configuration and state namespace, RCP model
discovery and reasoning choices, Ultra orchestration, Exa and generic MCP,
local plugins and skills, source installation and configuration import.
Voice remains a packaged native feature on Linux x64/ARM64 and macOS
Intel/Apple Silicon, with the same-build helper, pinned libraries, notices and
integrity manifest.

The refresh removes the new Statsig SDK and bootstrap paths, native Cloud
environment selection and recovery clients, and pet rendering. Terminal image
protocols remain available for coding transcripts. Account reporting,
workspace promotions, owner email nudges and credit reset operations are
rejected at the app-server boundary. Local coding token counts and saved
conversation history remain separate from those account products.
Bundled model instructions, diagnostic guidance and terminal tips use Lemex;
unused legacy prompt files are removed. Voice instructions focus on coding.

The history checks cover global browsing across projects and providers,
non-interactive exec chats, transcript replay and continuation after two process
restarts, and persisted input, cached-input, output, reasoning and total tokens.
`resume` remains scoped to the current project. Global browsing retains account
ownership checks and excludes background subagent source kinds.

Release gates include formatting and workspace checks, dependency audits,
provider and history regressions, source installer and package tests, and
main-branch CI. Every native npm installation must pass the relocated voice
runtime check and the private history fixture, including rejection of account
reporting and credit operations. Runtime initialization and an audio SDP offer
do not establish microphone/speaker availability or a live provider conversation.

The existing cryptography exception is reviewed in [npm-release.md](npm-release.md).
No new vulnerability exception is introduced. The final release receipt records
the verified source commit, package version, workflow URL, registry integrity
and clean installation results. Local Rust build/download caches are removed
after validation; installed tools, source, settings and saved chats are retained.
