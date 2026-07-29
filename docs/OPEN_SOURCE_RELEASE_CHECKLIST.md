# Open Source Release Checklist

## Legal

- Confirm the root Apache-2.0 license and workspace package metadata still
  match.
- Verify that no third-party code or assets require attribution or removal.
- Perform a release-wide composition/notices review when required by the
  project's broader release process. For external MCP integrations, start with
  [MCP open source review](MCP_OPEN_SOURCE_REVIEW.md).
- Confirm the README and repository metadata match the actual project state.

## Security

- Confirm no secrets are tracked in Git.
- Consider enabling GitHub private vulnerability reporting and verifying
  maintainer notifications.
- Review GitHub Actions access to signing and release secrets.
- Require review for workflow changes.
- Enable `Require review from Code Owners` for `main`.
- Create a protected `release` environment before adding signing secrets.
- Review local HTTP and pairing documentation before public release.

## Repository hygiene

- Remove temporary build artifacts.
- Remove stale or private-only documentation.
- Ensure public docs do not contain local machine paths.
- Keep only documentation that matches the public project story.

## Validation

- Run frontend tests.
- Run relevant builds.
- Run `cargo check`.
- Review the diff as if it were a first public impression of the repository.
- Walk through [docs/GITHUB_REPOSITORY_SETTINGS.md](docs/GITHUB_REPOSITORY_SETTINGS.md) before flipping the repository to public.
