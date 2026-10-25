# Publishing Lemex to npm

The `Lemex npm release` workflow builds precompiled CLI packages for macOS and
Linux on x64 and ARM64. Windows users install the Linux package inside WSL2.
End users need Node.js 22+ and npm, without Rust, Python, or a compiler.

All five tarballs use the single npm package name `@lemmy00/lemex`. The four native
versions have suffixes such as `0.1.0-linux-x64`; the launcher at `0.1.0` selects
the correct native version using npm's optional dependencies and OS/CPU filters.
Each native package includes the CLI, code-mode host, ripgrep, runtime metadata,
the same-build voice helper and pinned native audio libraries, and, on Linux,
the sandbox helper. Linux pairs the musl CLI with a GNU voice helper (glibc 2.28+).
The macOS voice runtime requires macOS 14+. No separate GStreamer installation
is needed. Microphone/speaker hardware and a provider supporting the realtime
voice API are required for a live conversation. Configuration and model defaults are embedded
in the CLI and installed by `lemex setup`.

## First release

1. Create a free account at <https://www.npmjs.com/signup>, verify your email,
   enable two-factor authentication, and run `npm login` locally. Choose a public
   email address you are comfortable associating with the package.
2. After this workflow is on the default branch, build without publishing:

   ```sh
   gh workflow run npm-release.yml --ref main -f version=0.1.0 -f publish=false
   gh run list --workflow npm-release.yml
   gh run watch RUN_ID --exit-status
   gh run download RUN_ID --name lemex-npm-release --dir dist/npm
   ```

3. Review the workflow's successful build and install checks. On Python 3.11+
   (use `python3.12` on macOS if the system `python3` is older), validate and
   publish the downloaded tarballs:

   ```sh
   python3 scripts/npm_release.py publish --version 0.1.0 --dist-dir dist/npm --dry-run
   python3 scripts/npm_release.py publish --version 0.1.0 --dist-dir dist/npm
   ```

   npm may open a browser to complete the publishing authentication challenge.
   Native versions publish first under `release-0.1.0`; the launcher publishes
   last under `latest`. A prerelease launcher uses `next`. The script checks that
   every expected tarball is present and complete before publishing anything.
   After a partial failure, rerun with the same tarballs: already published
   versions are accepted only when their npm integrity matches exactly.

4. Verify an installation from the public registry in a fresh directory:

   ```sh
   npm install --prefix /tmp/lemex-npm-check --registry=https://registry.npmjs.org/ @lemmy00/lemex@0.1.0
   /tmp/lemex-npm-check/node_modules/.bin/lemex --version
   /tmp/lemex-npm-check/node_modules/.bin/lemex setup --help
   ```

5. Once the first release is live and its public registry installation is verified,
   remove any first-release pending notice from the README and installation guide.

## Following releases

In the `@lemmy00/lemex` package's npm settings, add a GitHub Actions trusted publisher:

- Organization or user: `Lemmy00`
- Repository: `lemex`
- Workflow filename: `npm-release.yml`
- Allow direct `npm publish`.

Then launch the workflow with the new version and `publish=true`:

```sh
gh workflow run npm-release.yml --ref main -f version=0.1.1 -f publish=true
```

To publish the exact tarballs from an already successful build, supply its run ID:

```sh
gh workflow run npm-release.yml --ref main -f version=0.1.0 -f publish=true -f reuse_run_id=RUN_ID
```

Reuse requires a successful run of this workflow on the same repository and
branch, with unchanged native build inputs. The workflow preserves the original native
binaries, restages package metadata and the launcher from the current checkout,
audits dependencies again, and validates all five tarballs before publishing.
Voice helpers, libraries, notices and integrity manifests are preserved byte for
byte. Artifacts built before native voice packaging cannot be reused.

This uses npm's OIDC trusted publishing, with no npm token stored in GitHub.
Configure it when ready to publish: npm currently requires a new trusted
publisher to complete its first publication within two days. See
[npm trusted publishing](https://docs.npmjs.com/trusted-publishers/).

Each platform must pass an actual global npm installation in an isolated prefix,
using its compiled native package. The install check exercises the launcher,
exact optional-dependency aliases, OS/CPU selection, version, and setup/exec help.
Native helper unit tests and installed-client UDP/TCP negotiation tests also run
on each platform. The install check relocates the installed native package to a path containing spaces, checks
the voice helper build commit, initializes the packaged GStreamer libraries,
creates an audio SDP offer, and shuts the helper down. This does not open audio
devices or prove a live backend conversation. The public-release receipt seals
verified runtime bytes; it does not assert Apple signing or notarization.
The final assembly validates and dry-runs the complete release before the
optional publishing job runs. npm scans accepted uploads before they become
installable; an upload success or an initial 404 is not a completed publication.
The publishing job waits up to 30 minutes for all four native versions to become
publicly available with their exact artifact integrity before uploading the
launcher, then waits for the launcher too. This lets native scans run in parallel
and prevents moving `latest` while its dependencies are unavailable.

If availability times out, inspect npm's scan/review status and retain the same
version and artifacts. Do not republish accepted uploads just because they return
404 during scanning. See [npm's publication availability explanation](https://github.blog/changelog/2026-07-28-npm-publish-time-malware-scanning-and-dual-use-metadata/).

The workflow stamps the chosen version into the Cargo workspace and its local
lockfile entries without changing third-party dependency pins. The source npm
manifest is private so a direct publish from the checkout cannot accidentally
release a launcher without native packages; only staged packages are public.

## Recovering a failed platform build

A completed unsuccessful run can retain successful platform artifacts. Use the
same package version and select its run ID:

```sh
gh workflow run npm-release.yml --ref main -f version=0.1.2 -f publish=true -f recover_run_id=RUN_ID
```

Recovery validates the source repository, branch, workflow, completed run and
successful job for each retained artifact. Native source/build inputs must be
unchanged. Only missing platforms rebuild, from the original source commit;
helpers, runtime receipts and CLI build commits stay consistent. The complete
four-platform release is validated before publication. Recovery and full
artifact reuse are mutually exclusive. Voice and CLI compile in independent parallel jobs before a native installation
check. Jobs have a four-hour ceiling to accommodate cold Intel Mac builds; the
ceiling is not an expected duration. Successful native packages are retained
when another platform fails, so recovery does not rebuild completed platforms.

## Packaging regression checks

```sh
python3 -m unittest discover -s scripts -p 'test_npm_release.py'
```

These tests include a small executable fixture to verify npm's dependency
selection offline. Full compiled-binary checks run in the release workflow.

## Dependency audits

`Lemex checks` runs `pnpm audit` against the committed workspace lockfile. Run it
locally after changing Node dependencies, along with the SDK tests. Rust release
checks use `cargo deny`; keep its exceptions aligned with `.cargo/audit.toml`.

Workspace resolutions do not apply to dependencies installed by consumers of a
published npm package. Audit a fresh consumer installation separately. Version
0.1.4 removes the unrelated UltraCode search package added in 0.1.3, including
its Bun/TypeScript dependencies and vulnerable Java-parser Lodash pins. Codex-style
Ultra reasoning uses the native runtime and requires no third-party MCP package.

## Temporary security exception for 0.1.3 and 0.1.4

The release owner approved a temporary exception on 2026-10-25 for
[RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html).
The existing `crypto-bigint 0.5.5` dependency can leak secret-dependent timing
information. It is reached through `elliptic-curve 0.13` in age/hpke, AWS login,
RTC, and user verification. The 0.5 series has no patched release; the fixed
0.7.6+ series does not satisfy these parent dependencies' version constraints.

The dependency path was reviewed again for 0.1.4; the release owner requested
publication after being told this issue remains. The exception accepts the
reported risk; it does not fix the vulnerability.
It is recorded in both `codex-rs/deny.toml` and `codex-rs/.cargo/audit.toml` and
applies only to this advisory. Review it before the next release or by
2026-11-24, and remove it when the parent dependencies can use the patched
series. All other release security checks remain in force.
