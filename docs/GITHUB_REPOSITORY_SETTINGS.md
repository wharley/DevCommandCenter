# GitHub Repository Settings

This guide covers the GitHub settings that should be applied before turning the repository public and before publishing the first signed release.

## 1. Branch protection for `main`

Recommended path:

1. Open `Settings` -> `Branches`
2. Create or edit a protection rule for `main`
3. Enable `Require a pull request before merging`
4. Require at least `1` approval
5. Enable `Dismiss stale pull request approvals when new commits are pushed`
6. Enable `Require review from Code Owners`
7. Keep force-pushes disabled

Optional:

- Enable `Require status checks to pass before merging` only if you later reintroduce automatic validation workflows on push or pull request.

Notes:

- The repository now includes [`CODEOWNERS`](../.github/CODEOWNERS) for workflow and release-sensitive files.
- If you are the only maintainer, do not enable a combination that blocks you from merging urgent fixes with no fallback path.

## 2. Actions permissions

Recommended path:

1. Open `Settings` -> `Actions` -> `General`
2. Keep actions enabled for the repository
3. Set `Workflow permissions` to `Read repository contents permission`
4. Leave `Allow GitHub Actions to create and approve pull requests` disabled unless you have a specific automation that needs it

Why this matters:

- Validation workflows only need read access and artifact upload.
- The release workflow already declares `contents: write` explicitly for creating GitHub Releases.

## 3. Protected release environment

Recommended path:

1. Open `Settings` -> `Environments`
2. Create an environment named `release`
3. Add at least one required reviewer
4. Enable `Prevent self-review` only if you have another maintainer available to approve
5. Restrict deployment branches or tags to tags matching `v*`
6. Add the release secrets described in [docs/GITHUB_RELEASE_BOOTSTRAP.md](docs/GITHUB_RELEASE_BOOTSTRAP.md)

Why this matters:

- Only `.github/workflows/publish-release.yml` should touch release signing and publication.
- Environment review gives you a final approval gate before secrets are exposed to the runner.

## 4. First public-release dry run

Recommended order:

1. Merge the open-source cleanup branch into `main`
2. Turn the repository public
3. Apply branch protection and environment settings immediately
4. Add release secrets to the `release` environment
5. Run `.github/workflows/publish-release.yml` with `workflow_dispatch` first
6. Inspect the draft release assets and `latest.json`
7. Only then create and push the first public tag such as `v0.1.1`

Why dispatch first:

- It validates the signing and notarization plumbing without forcing a permanent public tag immediately.
- It is the safest way to validate the current macOS/Linux-only public distribution path.

## 5. Release rollback posture

Before the first public release, make sure you know who will do each of these:

- revoke or rotate Apple credentials if a CI secret is exposed
- disable the release workflow if a malicious change lands in `.github/workflows/`
- publish a corrected GitHub Release if `latest.json` is missing or malformed
