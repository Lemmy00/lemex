# Workflow Strategy

## Lemex

On this fork, `blocking-ci.yml` runs on pull requests and pushes to `main`:

- `lemex-ci.yml`: Rust formatting, manifest and architecture checks, workflow
  validation, native/npm packaging tests, GitHub release-helper tests, and a
  locked Linux Cargo check when Rust or runtime configuration changes.
- `source-installer.yml`: installer and npm regression tests on standard Linux
  and macOS GitHub-hosted runners, called by the Lemex suite.
- `cargo-deny.yml`: dependency security, license, and source checks.
- `codespell.yml` and `blob-size-policy.yml`: spelling and changed-file size.

`CI required` explicitly requires every applicable job to succeed. Missing,
failed, cancelled, or unexpectedly skipped required jobs fail the gate. The
upstream Bazel, SDK, and custom-runner suites are excluded from the fork's
automatic gate because they require infrastructure not configured here. The
original workflows remain available for manual use; `postmerge-ci` runs the
full upstream suites on this fork only when dispatched manually.

`npm-release.yml` separately requires native builds and npm installation tests
on Linux and macOS, each on x64 and ARM64, before assembling all five tarballs.
Publishing is opt-in and depends on the successful assembly. The first release
is published locally with npm authentication; following releases can use an
npm trusted publisher. See [npm release instructions](../../docs/npm-release.md).

Ordinary main-branch pushes cancel superseded `blocking-ci` runs. Release runs
are serialized without cancellation so an in-progress publication is preserved.

## Upstream reference

The following describes the retained OpenAI CI configuration. It applies to
`openai/codex`, rather than the default Lemex gate.

The workflows in this directory are split so that pull requests get fast, review-friendly signal while `main` still gets the full cross-platform verification pass.

## Pull Requests

- Required checks run against GitHub's synthetic merge commit, not the pull
  request head alone. This includes changes already on `main` and catches
  conflicts before they reach the branch.
- `bazel.yml` is the main pre-merge verification path for Rust code.
  It runs Bazel `test` and Bazel `clippy` on the supported Bazel targets,
  including the generated Rust test binaries needed to lint inline `#[cfg(test)]`
  code.
- `rust-ci.yml` keeps the Cargo-native PR checks intentionally small:
  - `cargo fmt --check`
  - `cargo shear`
  - `argument-comment-lint` on Linux, macOS, and Windows
  - `tools/argument-comment-lint` package tests when the lint or its workflow wiring changes

## Post-Merge On `main`

- `bazel.yml` also runs on pushes to `main`.
  This re-verifies the merged Bazel path and helps keep the BuildBuddy caches warm.
- `rust-ci-full.yml` is the full Cargo-native verification workflow.
  It keeps the heavier checks off the PR path while still validating them after merge:
  - the full Cargo `clippy` matrix
  - the full Cargo `nextest` matrix via per-platform archive-backed shards
  - Windows ARM64 nextest archives cross-compiled on Windows x64, then replayed on native Windows ARM64 shards
  - release-profile Cargo builds
  - cross-platform `argument-comment-lint`
  - Linux remote-env tests

## Rule Of Thumb

- If a build/test/clippy check can be expressed in Bazel, prefer putting the PR-time version in `bazel.yml`.
- Keep `rust-ci.yml` fast enough that it usually does not dominate PR latency.
- Reserve `rust-ci-full.yml` for heavyweight Cargo-native coverage that Bazel does not replace yet.
