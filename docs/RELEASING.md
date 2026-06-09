# Releasing

This repository is configured to use GitHub Releases as the first public distribution channel for desktop builds.

Current public release scope:

- macOS
- Linux (AppImage in the public release pipeline)

## Release model

- Manual validation workflows build and upload temporary artifacts when you explicitly dispatch them.
- The `publish-release.yml` workflow creates or updates a GitHub Release draft.
- The Tauri updater is configured to read `latest.json` from the latest GitHub Release.
- Linux public releases build `AppImage` only, which matches the artifact Tauri uses for updater support on Linux.

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

## Publish flow

1. Update the application version.
2. Push a tag in the format `v<version>` or trigger the publish workflow manually.
3. For `workflow_dispatch`, choose `all`, `linux-x64`, `macos-arm64`, or `macos-intel` depending on what you want to validate.
4. Let the workflow build the selected bundles plus updater artifacts.
5. Review the generated GitHub Release draft.
6. Publish the draft once the assets are validated.

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
- macOS release builds depend on Apple signing and notarization secrets being configured correctly.
