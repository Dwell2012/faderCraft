# Installers and updates

Published GitHub Releases at <https://github.com/Dwell2012/faderCraft/releases> are
the single source of truth for downloadable versions. The app's **Settings →
About → Check for updates** button reads GitHub's latest published release,
compares its `vMAJOR.MINOR.PATCH` tag with the installed Cargo version, and opens
the release page when a newer version exists. It never replaces a running binary.

Pushing a version tag such as `v0.2.0` builds a Windows per-user installer,
Intel and Apple silicon macOS `.dmg` files, and x86-64 and ARM64 Linux `.deb`
and `.tar.gz` files. The workflow checks that the
tag matches `[workspace.package].version` and belongs to `main`, then uploads
the packages as workflow artifacts. SHA-256 checksum files accompany them.
Only after every platform package succeeds does the publish job create the
GitHub Release with all assets. `workflow_dispatch` builds packages without
publishing a release, for installer testing.

Windows uses Inno Setup. Installing the next version over the same AppId
replaces application files; uninstalling removes application files. The app's
library and settings live outside the install directory and are retained.
The macOS disk image currently contains an unsigned, unnotarized app bundle,
so it is suitable for local testing; signing and notarization are needed for
a frictionless public macOS installation. The Debian packages and archives target
Ubuntu-compatible x86-64 and ARM64 distributions, including 64-bit Raspberry Pi OS
where its runtime dependencies are available. Other Linux formats remain future work.

The release job is gated by repository variable
`ASIO_DISTRIBUTION_APPROVED=true`. Windows builds use the Steinberg ASIO SDK.
Before setting that variable, confirm the distribution agreement described in
[ASIO.md](ASIO.md#sdk-licensing). Until then, the workflow produces reviewable
artifacts but does not publish a public release. Do not treat successful CI or
Cargo's Rust licence check as ASIO distribution clearance.

Release procedure:

1. Bump the workspace version and update `Cargo.lock`; merge to `main`.
2. Run CI and a manual `workflow_dispatch` package build. Test installation,
   update over an older version, retained library/settings, and uninstall.
3. Once ASIO distribution is cleared, set the repository variable, tag the
   commit `v<version>`, and push the tag. Confirm all assets and checksums are
   attached to the published GitHub Release.
