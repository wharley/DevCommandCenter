# Authenticated Delivery Smoke Tests

These opt-in tests exercise the real DCC forge adapters against fixtures that
already exist on GitHub or GitLab. They are ignored by the default test suite,
do not accept credentials as fixture variables, and do not print provider
payloads or job-log contents. Authentication stays with the provider CLI.

Use a dedicated, disposable fixture when possible. Run one smoke test at a
time so the selected CLI account and fixture are unambiguous.

## Prerequisites

- Authenticate the required account with `gh` or `glab`.
- Confirm that the selected account can read the fixture.
- Export fixture identifiers only. Do not export tokens.
- Keep the PR/MR open for fork and review-rule checks.

The optional login variables make DCC verify the exact active CLI identity:

- `DCC_SMOKE_GITHUB_LOGIN`;
- `DCC_SMOKE_GITLAB_LOGIN`.

## GitHub fork PR

The target repository must use `owner/repository`.

```sh
DCC_SMOKE_GITHUB_PR_URL=https://github.com/acme/project/pull/123 \
DCC_SMOKE_GITHUB_TARGET_REPOSITORY=acme/project \
DCC_SMOKE_GITHUB_LOGIN=contributor \
cargo test -p dcc-tauri authenticated_smoke_github_fork_pull_request \
  -- --ignored --nocapture
```

The test requires an open PR and verifies that the head repository differs
from the configured target repository.

## GitLab open MR

All GitLab tests use the following fixture variables:

```sh
DCC_SMOKE_GITLAB_REPOSITORY=group/project
DCC_SMOKE_GITLAB_MR_IID=123
DCC_SMOKE_GITLAB_LOGIN=contributor
```

Run the read-only baseline:

```sh
DCC_SMOKE_GITLAB_REPOSITORY=group/project \
DCC_SMOKE_GITLAB_MR_IID=123 \
DCC_SMOKE_GITLAB_LOGIN=contributor \
cargo test -p dcc-tauri authenticated_smoke_gitlab_open_merge_request \
  -- --ignored --nocapture
```

## GitLab fork MR

```sh
DCC_SMOKE_GITLAB_REPOSITORY=group/project \
DCC_SMOKE_GITLAB_MR_IID=123 \
DCC_SMOKE_GITLAB_LOGIN=contributor \
cargo test -p dcc-tauri authenticated_smoke_gitlab_fork_merge_request \
  -- --ignored --nocapture
```

The test requires different source and target project IDs and verifies that
DCC can resolve the source repository and clone URL.

## GitLab review rules

```sh
DCC_SMOKE_GITLAB_REPOSITORY=group/project \
DCC_SMOKE_GITLAB_MR_IID=123 \
DCC_SMOKE_GITLAB_LOGIN=contributor \
cargo test -p dcc-tauri authenticated_smoke_gitlab_review_rules \
  -- --ignored --nocapture
```

The MR must expose an approval rule with at least one required approval.

## GitLab pipeline jobs and bounded logs

`DCC_SMOKE_GITLAB_JOB_ID` is optional for the read-only test. Without it, DCC
selects the first completed, non-archived job.

```sh
DCC_SMOKE_GITLAB_REPOSITORY=group/project \
DCC_SMOKE_GITLAB_PIPELINE_ID=456 \
DCC_SMOKE_GITLAB_JOB_ID=789 \
DCC_SMOKE_GITLAB_LOGIN=contributor \
cargo test -p dcc-tauri authenticated_smoke_gitlab_pipeline_job_log \
  -- --ignored --nocapture
```

The test verifies pipeline identity, retained jobs, retry eligibility, the
256-KiB log bound, and removal of terminal and GitLab section markers. It does
not print the log.

## GitLab job retry

This is the only mutating smoke test. It is separate from every read-only test,
requires an exact pipeline and job ID, verifies that the job belongs to that
pipeline, and requires the explicit confirmation sentinel below.

```sh
DCC_SMOKE_GITLAB_REPOSITORY=group/project \
DCC_SMOKE_GITLAB_PIPELINE_ID=456 \
DCC_SMOKE_GITLAB_JOB_ID=789 \
DCC_SMOKE_GITLAB_LOGIN=contributor \
DCC_SMOKE_GITLAB_ALLOW_RETRY=retry-this-exact-job \
cargo test -p dcc-tauri authenticated_smoke_gitlab_job_retry \
  -- --ignored --nocapture
```

Never run the retry test against a production pipeline or from unattended CI.
