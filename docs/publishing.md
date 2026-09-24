# Publishing

## Applications

Set the app ID, version, web build, and update source in `Sabine.toml`. Initialize signing and the reusable release workflow from the app repository:

```sh
sabine release-init --repository owner/repository
```

Review the generated workflow and public key, then commit the app and push a tag matching its version, such as `v1.0.0`. The workflow builds and signs the platform packages and update manifest. Keep the signing key in the repository's Actions secret; installed apps receive only its public key. The [implementation guide](implementation-guide.md#release-and-update-model) explains verification, rollout, and rollback.

MSI builds use WiX 7. After reviewing the [WiX EULA](https://docs.firegiant.com/wix/osmf/), set `accept_wix_eula: true` in the reusable workflow input. Local build machines can run `wix eula accept wix7`. Windows EXE bundles use NSIS.

## Sabine itself

Use the release script from a clean, synchronized `main` branch:

```sh
scripts/publish.sh --dry-run
scripts/publish.sh --prepare
scripts/publish.sh
```

The dry run previews version edits without changing files or GitHub state. `--prepare` writes those edits and runs local checks without committing, tagging, or pushing; review and commit them before publishing. With no version argument, the script selects the next build or resumes the prepared current build.

Publication updates version references, runs format, build, test, and Clippy checks, signs and pushes the release commit, waits for CI, signs and pushes the tag, and monitors artifact publication. It never replaces an existing tag. The release initially remains outside GitHub's `latest` channel during the normal soak period. See the [implementation guide](implementation-guide.md#release-and-update-model) for the shared-system update policy.

The changelog retains release history; each GitHub release page shows only its own version section.
