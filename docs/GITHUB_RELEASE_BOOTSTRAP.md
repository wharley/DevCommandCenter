# GitHub Release Bootstrap

This guide is the operational checklist for turning on signed public releases from GitHub Actions.

## 1. Create the updater signing key

Tauri requires signed updater artifacts. Generate the key pair locally and keep the private key outside the repository.

```bash
yarn tauri signer generate -w ~/.tauri/dev-command-center.key
```

According to the Tauri updater documentation, the private key must be provided at build time through environment variables and `.env` files do not work for this flow.

## 2. Update or confirm the public key

The public key is safe to publish and must match the private key used by CI.

Check the public key:

```bash
cat ~/.tauri/dev-command-center.key.pub
```

Confirm that the value in [`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) matches that public key exactly.

## 3. Create the `release` environment on GitHub

Recommended setup:

1. Open `Settings` -> `Environments`
2. Create an environment named `release`
3. Add at least one required reviewer
4. Enable `Prevent self-review` if you want stricter release approval
5. Restrict deployment refs to tags matching `v*`
6. Add the environment secrets below

The publish workflow now targets the `release` environment explicitly.

## 4. Add environment secrets

Recommended: add these as **environment secrets** on `release`, not plain repository secrets.

Updater signing:

```bash
gh secret set --env release TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/dev-command-center.key
gh secret set --env release TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

If the key has no password, leave `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` unset or set it to an empty value in the GitHub UI.

macOS signing and notarization:

```bash
gh secret set --env release APPLE_CERTIFICATE
gh secret set --env release APPLE_CERTIFICATE_PASSWORD
gh secret set --env release APPLE_SIGNING_IDENTITY
gh secret set --env release APPLE_TEAM_ID
gh secret set --env release APPLE_ID
gh secret set --env release APPLE_PASSWORD
```

Notes:

- `APPLE_CERTIFICATE` is typically the base64-encoded `.p12` content.
- Keep the raw certificate file out of the repository and local shell history when possible.

## 5. Keep validation workflows separate from publishing

- Validation workflows can stay on normal repository permissions and artifact uploads.
- The publish workflow should remain the only workflow that touches release signing and updater publication.
- Review changes to `.github/workflows/` carefully before merging.

## 6. First release test

1. Confirm the version in [`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) and package metadata is the version you want to release.
2. Commit and push `main`.
3. Recommended first pass: trigger `.github/workflows/publish-release.yml` with `workflow_dispatch`, choose the platform you want to validate first, and confirm the draft release looks correct.
4. After that, create a tag:

```bash
git tag v0.1.1
git push origin v0.1.1
```

5. Wait for `.github/workflows/publish-release.yml` to finish.
6. Open the generated GitHub Release draft.
7. Confirm that the assets include updater metadata such as `latest.json` and signature files.
8. Publish the draft only after checking at least one downloaded build manually.

For Linux, expect the public release workflow to publish an `AppImage`. The separate Linux validation workflow can still be used when you want to build a `.deb` artifact manually.

## 7. Post-release verification

- Visit:

```text
https://github.com/wharley/DevCommandCenter/releases/latest/download/latest.json
```

- Confirm the JSON exists and contains platform entries for the released artifacts.
- Install the previous app version locally and verify that the updater detects the new release.

## 8. Recovery notes

- If you lose the updater private key, existing installed clients will not trust future updates signed by a different key without additional migration work.
- If a bad release becomes the latest GitHub Release without `latest.json` or valid signatures, updater checks may fail for users until a corrected release replaces it.
- Forks should not reuse this repository's release endpoint or signing identity. Each fork that wants downloadable builds should configure its own keys, secrets, and GitHub Releases URL.
