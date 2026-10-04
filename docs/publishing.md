# Publishing

## Applications

Set the app ID, version, web build, and update source in `Sabine.toml`. Initialize signing and the reusable release workflow from the app repository:

```sh
sabine release-init --repository owner/repository
```

Review the generated workflow and public key, then commit the app and push a tag matching its version, such as `v1.0.0`. The workflow builds and signs the platform packages and update manifest. Keep the signing key in the repository's Actions secret; installed apps receive only its public key. The [implementation guide](implementation-guide.md#release-and-update-model) explains verification, rollout, and rollback.

The workflow builds every package an installed app can update from:

| Platform | Packages |
| --- | --- |
| Linux x86_64 and ARM64 | AppImage, deb, rpm, and a tarball for apps installed with `sabine install --bundle` |
| Windows x86_64 and ARM64 | MSI, setup `.exe`, and a tarball for `sabine install --bundle` |
| macOS Apple Silicon | DMG, and a tarball for `sabine install --bundle` |

Each installation updates from the same kind of package it was installed with.

MSI builds use WiX 7. After reviewing the [WiX EULA](https://docs.firegiant.com/wix/osmf/), set `accept_wix_eula: true` in the reusable workflow input. Local build machines can run `wix eula accept wix7`. Windows EXE bundles use NSIS.

### Code signing

Unsigned packages build and update normally, but macOS Gatekeeper and Windows SmartScreen warn people before they open them. `sabine bundle` signs whatever the environment configures:

| Variable | Purpose |
| --- | --- |
| `SABINE_MACOS_SIGNING_IDENTITY` | Developer ID Application identity in the keychain; signs `.app` bundles and disk images with the hardened runtime |
| `SABINE_NOTARY_APPLE_ID`, `SABINE_NOTARY_TEAM_ID`, `SABINE_NOTARY_PASSWORD` | Notarize and staple disk images; set all three, with an app-specific password |
| `SABINE_WINDOWS_CERTIFICATE` | Path to a `.pfx` Authenticode certificate; signs every `.exe` in the app and the MSI or setup `.exe` |
| `SABINE_WINDOWS_CERTIFICATE_PASSWORD` | Password of that certificate |
| `SABINE_WINDOWS_TIMESTAMP_URL` | RFC 3161 timestamp server, `http://timestamp.digicert.com` by default |

Signing uses `codesign`, `notarytool` and `stapler` from Xcode, and `signtool` from the Windows SDK.

The reusable workflow sets these from optional repository secrets: `MACOS_CERTIFICATE` (a base64 `.p12`), `MACOS_CERTIFICATE_PASSWORD`, `MACOS_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`, `WINDOWS_CERTIFICATE` (a base64 `.pfx`) and `WINDOWS_CERTIFICATE_PASSWORD`. Projects created with `sabine new` pass their repository secrets with `secrets: inherit`. Platforms without their secrets are built unsigned.

## Sabine itself

Use the release script from a clean, synchronized `main` branch:

```sh
scripts/publish.sh --dry-run
scripts/publish.sh --prepare
scripts/publish.sh
```

The dry run previews version edits without changing files or GitHub state. `--prepare` writes those edits and runs local checks without committing, tagging, or pushing; review and commit them before publishing. With no version argument, the script selects the next build or resumes the prepared current build.

The shared system runs one native host for every app. When the host protocol has changed since the last release, preparation raises the oldest supported app build to the new build, so apps built against an earlier host receive the incompatibility notice until they update.

Publication updates version references, runs format, build, test, and Clippy checks, signs and pushes the release commit, waits for CI, signs and pushes the tag, and monitors artifact publication. It never replaces an existing tag. The release initially remains outside GitHub's `latest` channel during the normal soak period. See the [implementation guide](implementation-guide.md#release-and-update-model) for the shared-system update policy.

The changelog retains release history; each GitHub release page shows only its own version section.
