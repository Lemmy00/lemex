# Rama DNS security dependency update

Source: `https://static.crates.io/crates/rama-dns/rama-dns-0.3.0-alpha.4.crate`
SHA-256: `e340fef2799277e204260b17af01bc23604712092eacd6defe40167f304baed8` (verified against the crates.io version record).
License: MIT OR Apache-2.0; license files are copied from the matching
upstream `rama-0.3.0-alpha.4` tag.

This copy preserves the pinned Rama 0.3.0-alpha.4 DNS interface and uses
Hickory 0.26.3, including the fixes for RUSTSEC-2026-0487, 0489, 0490 and 0491.
The other Rama dependencies remain pinned to the current interface.
The resolver adapter is adjusted only where Hickory's updated API requires it.

Remove this patch when the network proxy can adopt a parent Rama release with
the patched resolver. Validate the DNS and complete network-proxy regression
suites whenever changing this adapter.
