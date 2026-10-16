# Releasing

This repository is configured to use GitHub Releases as the first public distribution channel for desktop builds.

Current public release scope:

- macOS
- Linux (Debian package in the public release pipeline)

## Release model

- Manual validation workflows build and upload temporary artifacts when you explicitly dispatch them.
- The `publish-release.yml` workflow creates or updates a GitHub Release draft.
- The Tauri updater is configured to read `latest.json` from the latest GitHub Release.
- macOS public releases build signed/notarized DMG installers and signed
  app bundle archives (`.app.tar.gz`). The DMG is the direct-download
  installer; the archive and its `.sig` remain updater artifacts.
- Linux public releases build `.deb` packages in the GitHub release pipeline.

## Required secrets

Updater signing:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (optional, only if your private key uses a password)

macOS signing and notarization:

- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_TEAM_ID`
- `APPLE_ID`
- `APPLE_PASSWORD`

## First-time setup

1. Generate and store the Tauri updater signing key pair in a safe place.
2. Keep the private key out of the repository and Git history.
3. Add the public key to `src-tauri/tauri.conf.json`.
4. Create and protect the `release` GitHub environment.
5. Configure the GitHub environment secrets listed above.
6. Follow the bootstrap checklist in [docs/GITHUB_RELEASE_BOOTSTRAP.md](docs/GITHUB_RELEASE_BOOTSTRAP.md).
7. Run the publish workflow manually or push a tag like `v0.1.1`.

## Use a local build before releasing (macOS)

Run the next version as an installed production app, on the real data, for
as long as it takes to trust it, then cut the release:

```bash
yarn app:local
```

1. It builds a release bundle (`--bundles app`) versioned
   `<next patch>-local.g<commit>`, for example `0.1.99-local.g677c33b`, with
   `.dirty` appended when tracked files have uncommitted changes. It skips
   notarization and the updater archive, and signs with the keychain's
   `Developer ID Application` identity when there is one, so macOS keeps
   granted permissions between builds.
2. It quits Dev Command Center and stops every helper running from the
   installed bundle. `dccd` and `ai-memory` outlive the window, and the new
   app would otherwise keep using the old daemon binary.
3. It snapshots the database (APFS clones of `database.sqlite`, `-wal` and
   `-shm`) and the app it replaces into `~/.dcc/local-builds/snapshots/`,
   keeping the last three.
4. It installs the build into `/Applications` and opens it. Settings shows the
   local version.

`yarn app:local --skip-build` reinstalls the last build. `yarn app:rollback`
puts back the app the last install replaced; `--with-db` also restores the
database as it was right before that install, which drops everything recorded
since. The app and database being replaced go to
`~/.dcc/local-builds/rolled-back/`.

The local version sorts above the published release and below the next one,
so the updater never offers a downgrade and swaps the local build for the
real release once it is published. The build uses the release data directory
(`com.devcommandcenter.app`), unlike `yarn dev` (`com.devcommandcenter.app.dev`).
Do not run `yarn dev` alongside the installed app: both bind the local HTTP
API port (9876) and the quick-composer shortcut.

## Publish flow

1. Update the application version.
2. Push a tag in the format `v<version>` or trigger the publish workflow manually.
3. For `workflow_dispatch`, choose `all`, `linux-x64`, `macos-arm64`, or `macos-intel` depending on what you want to validate.
4. Let the workflow build the selected bundles plus updater artifacts. macOS
   targets are built explicitly as ARM64 and Intel, each with `app,dmg`.
   Tauri notarizes and staples only the `.app`; the DMG comes out signed but
   without a ticket, so `scripts/notarize-macos-dmg.sh` submits it to Apple,
   staples it, and the job replaces the draft's DMG asset with the stapled one.
   The release matrix is deliberately serialized (`max-parallel: 1`) because
   each `tauri-action` run updates the same draft release and can otherwise race
   while listing, deleting, and uploading `latest.json`.
5. Wait for every matrix job to pass. The macOS job blocks on Developer ID
   signature validation for the app and DMG, stapled notarization tickets for
   both, Gatekeeper assessment of the DMG, and presence of the updater archive
   signature.
6. For a release candidate (tag or manual `all`), review the generated GitHub
   Release draft. For each macOS architecture it must include a `.dmg`, an
   `.app.tar.gz`, its `.sig`, and the release's `latest.json` updater metadata.
   It must also carry version-free copies of the installers, which the landing
   page links to through `releases/latest/download/`:
   `Dev.Command.Center_aarch64.dmg`, `Dev.Command.Center_x64.dmg` and
   `Dev.Command.Center_amd64.deb`. Renaming or dropping them breaks the site's
   download buttons.
7. Publish the draft only after all jobs pass. `tauri-action` sends assets to a
   draft before the local macOS validation gate runs; if that gate fails,
   discard the draft and rerun the workflow rather than publishing or repairing
   it manually.

### Partial manual dispatches are diagnostics only

A manual target other than `all` is useful for validating one platform, but it
does not upload `latest.json` and its draft must never be published. This
prevents a single-platform run from replacing the complete cross-platform
updater metadata. Use a version tag or a manual `all` dispatch for a release
candidate, then publish only after its serialized matrix succeeds.

## Public download URLs

- Release page:

```text
https://github.com/wharley/DevCommandCenter/releases/latest
```

- Updater metadata used by the app:

```text
https://github.com/wharley/DevCommandCenter/releases/latest/download/latest.json
```

Notes:

- The SHA-256 digest shown by GitHub Actions artifact upload steps is not a public download URL. It is only a checksum for the temporary workflow artifact created in that run.
- For public distribution, use GitHub Releases assets and the release URLs above.
- If someone forks this repository and wants their own signed downloads, they should use their own signing keys, their own Apple credentials, and their own GitHub Releases endpoint.

## Notes

- The updater requires signed artifacts. This cannot be skipped.
- The endpoint in `tauri.conf.json` only works after the first release containing `latest.json`.
- If a release is created without updater artifacts, using `releases/latest/download/latest.json` may break update checks for existing users.
- macOS release builds depend on Apple signing and notarization secrets being configured correctly. Tauri must receive a `Developer ID Application` identity, rather than an ad-hoc identity, for public distribution.
- The workflow uses Tauri's default app notarization/stapling flow (it does not
  pass `--skip-stapling`), then notarizes and staples the DMG itself, and
  validates both with `codesign`, `xcrun stapler`, and (for the DMG)
  `spctl --context context:primary-signature` before its job can pass.
- To test the DMG signing/notarization path without touching any GitHub
  Release, dispatch the manual **Build macOS** workflow
  (`build-macos-staging-to-main.yml`). It uses the `release` environment's
  secrets and uploads the `.app` and stapled `.dmg` only as workflow artifacts.
