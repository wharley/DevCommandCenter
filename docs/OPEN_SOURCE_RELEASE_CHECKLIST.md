# Open Source Release Checklist

## Legal

- Choose and add the project license.
- Verify that no third-party code or assets require attribution or removal.
- Confirm the README and repository metadata match the actual project state.

## Security

- Confirm no secrets are tracked in Git.
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
