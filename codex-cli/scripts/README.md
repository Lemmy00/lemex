# npm releases

For Lemex CLI releases, use the dedicated [npm release workflow](../../docs/npm-release.md).
It builds and tests macOS and Linux packages without upstream signing services.

The older multi-package staging helper below is retained for upstream tooling;
it depends on `rust-release` artifacts and is not the Lemex CLI release path.

Use the staging helper in the repo root to generate npm tarballs for a release. For
example, to stage the CLI, responses proxy, and SDK packages for version `0.6.0`:

```bash
./scripts/stage_npm_packages.py \
  --release-version 0.6.0 \
  --package lemex \
  --package lemex-responses-api-proxy \
  --package lemex-sdk
```

This downloads the required native package archive artifacts, hydrates `vendor/` for
each package, and writes tarballs to `dist/npm/`.

When `--package lemex` is provided, the staging helper builds the lightweight
`lemex` meta package plus all platform-native `lemex` variants
that are later published under platform-specific dist-tags.

Direct `build_npm_package.py` invocations are still useful for package-specific
debugging, but native packages expect `--vendor-src` to point at a prehydrated
`vendor/` tree. Release packaging should use `scripts/stage_npm_packages.py`
