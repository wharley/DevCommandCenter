# Releasing

Dev Command Center ships through GitHub Releases. The app is the native one
(`apps/native`, Rust and GPUI); the React/Tauri desktop it replaced is gone.

Current public release scope:

- macOS, Apple silicon and Intel (notarized DMG, plus the updater archive)
- Linux x86_64 (Debian package)

## Release model

- `publish-release.yml` builds every platform. A tag `v<version>` ends with a
  **draft** release; a manual run only builds and verifies (workflow artifacts,
  no release).
- Each release carries `latest.json`, the updater feed. The native app reads
  it (`apps/native/src/data/updater.rs`), and so does a v0.2.0 Tauri desktop:
  its own updater installs the native app over itself, so people on the old
  app move with one click on "Install".
- Every updater archive (`.app.tar.gz`, `.deb`) is signed with the updater's
  minisign key; both updaters refuse anything else.

## Required secrets

Updater signing (the same key the Tauri desktop shipped with):

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (only if the key has a password)

macOS signing and notarization:

- `APPLE_CERTIFICATE` (base64 of the Developer ID `.p12`)
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_TEAM_ID`
- `APPLE_ID`
- `APPLE_PASSWORD` (app-specific password)

## First-time setup

1. Keep the updater's private key out of the repository and Git history.
2. Its public key lives in `apps/native/src/data/updater.rs` (`PUBLIC_KEY`).
   Changing the key strands every installed app on its current version.
3. Create and protect the `release` GitHub environment with the secrets above.
4. Follow the bootstrap checklist in
   [docs/GITHUB_RELEASE_BOOTSTRAP.md](GITHUB_RELEASE_BOOTSTRAP.md).

## Use a local build before releasing (macOS)

Run the next version as the installed app, on the real data, for as long as it
takes to trust it, then cut the release:

```bash
yarn native:local --version 0.3.0
```

1. It builds the helpers (`scripts/build-sidecars.mjs`) and the app in release,
   bundles `Dev Command Center.app` (`com.devcommandcenter.app`) versioned
   `<version>-local.g<commit>` (`.dirty` with uncommitted tracked changes),
   and signs it with the keychain's `Developer ID Application` identity, so
   macOS keeps granted permissions between builds.
2. It quits the app and stops every helper running from its bundle.
3. It keeps the replaced build and a copy of the database in
   `~/.dcc/native-builds/snapshots/` (the last three).
4. It installs into `/Applications` and opens it.

`--skip-build` reinstalls the last build; `--skip-sidecars` reuses the built
helpers. `yarn native:rollback` puts back the build the last install replaced;
`--with-db` also restores the database as it was right before it.

A local version sorts below its release (`0.3.0-local.g…` < `0.3.0`), so once
the release is out the updater offers it and replaces the local build.

## Publish flow

1. On `main`, bump the version: `apps/native/Cargo.toml` (the source of the
   release version), `package.json`, `sidecar/package.json`,
   `sidecar/src/index.mjs` and `Cargo.lock`/`apps/native/Cargo.lock`; add
   `docs/releases/v<version>.md` (the release notes and `latest.json` notes).
2. Push a tag `v<version>`. The `version` job fails when the tag and
   `apps/native/Cargo.toml` disagree.
3. Each macOS job builds and signs the app (`native-local.mjs build --release`,
   secure timestamp), notarizes and staples it (`scripts/notarize-macos.sh`),
   launches it for ten seconds (`scripts/verify-macos-bundle.py`), packs the
   updater archive with a single `Dev Command Center.app` at its top, signs it,
   and builds, signs and notarizes the DMG.
4. The Linux job builds `dev-command-center` (`scripts/package-linux.mjs`, the
   Tauri package's own name, so `dpkg -i` replaces it) and signs the `.deb`.
5. The publish job writes `latest.json` (`scripts/release-latest-json.mjs`)
   and creates the draft with every asset. Check that it holds, per macOS
   architecture, the `.dmg`, the `.app.tar.gz` and its `.sig`; the `.deb` and
   its `.sig`; `latest.json`; and the version-free copies the landing page
   links through `releases/latest/download/`: `Dev.Command.Center_aarch64.dmg`,
   `Dev.Command.Center_x64.dmg` and `Dev.Command.Center_amd64.deb`.
6. Publish the draft only after every job passed. On a failure, delete the
   draft and run again rather than repairing it by hand.

To try the signing and notarization path without a release, run the workflow
manually: it builds and verifies, and keeps the files as workflow artifacts.

## Public download URLs

- Release page: `https://github.com/wharley/DevCommandCenter/releases/latest`
- Updater feed: `https://github.com/wharley/DevCommandCenter/releases/latest/download/latest.json`

The SHA-256 digest an artifact upload step prints is a checksum of the
temporary workflow artifact, not a download URL. A fork that wants its own
signed downloads needs its own updater key, Apple credentials and releases.

## Notes

- The updater requires signed archives; this cannot be skipped.
- A published release without `latest.json` breaks update checks for everyone
  (`releases/latest/download/latest.json` would point at nothing).
- Before the first native release, try the move end to end: install the v0.2.0
  Tauri desktop, point it at the candidate release and let it update.
