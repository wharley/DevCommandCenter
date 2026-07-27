# CodeRabbit Integration

Dev Command Center can run CodeRabbit reviews from the desktop app and use
the findings as context for follow-up work in the Composer.

The integration is intentionally review-only: DCC does not apply CodeRabbit
fixes automatically. The user chooses which findings to send to the Composer
and stays in control of any code changes.

## Requirements

- A working Git workspace.
- The CodeRabbit CLI installed and available as `cr` or `coderabbit`.
- A CodeRabbit account authenticated through the CLI.

Use the official CodeRabbit CLI documentation for installation and account
setup:

- <https://docs.coderabbit.ai/cli>
- <https://docs.coderabbit.ai/cli/reference>

## Connecting the CLI

Open `Settings > Account` and check the `CodeRabbit CLI` card. DCC detects
the CLI, reads its version, and checks whether the CLI is authenticated.

If authentication is required, use the connect action. DCC opens an embedded
terminal session for the normal CodeRabbit CLI login flow.

The same card has a **Show in Inspector** switch. Turning it off hides
CodeRabbit review prompts, summaries, panels, and file annotations without
changing the CLI login or deleting saved review history. Use **Disconnect**
when you also want DCC to run `cr auth logout` and clear CodeRabbit's local
authentication; disconnecting turns the Inspector integration off as well.

## Running a Review

Open a workspace and go to `Inspector > Changes`. The CodeRabbit review panel
appears with the Git changes for the workspace.

Available review scopes:

- `All`: review committed and uncommitted changes.
- `Uncommitted`: review staged, unstaged, and untracked changes.
- `Committed`: review committed branch changes.

After a review finishes, DCC stores the latest result for that workspace and
keeps recent review history locally. If the Git diff changes after a review,
the UI marks the saved result as stale so the old findings are not mistaken
for the current state.

## Privacy

CodeRabbit reviews are external reviews. Running a review sends code diff
content from the selected workspace to CodeRabbit through the CodeRabbit CLI.

DCC requires explicit opt-in before the first CodeRabbit review in each
workspace. The consent is local to the desktop app and can be reset by clearing
the app data for that workspace.

Review snapshots and history are stored locally by DCC so the Inspector can
show the last result and recent runs. Authentication remains owned by the
CodeRabbit CLI.

## Using Findings

CodeRabbit findings appear grouped by severity. Selecting a finding opens the
related file in the diff preview when DCC can match the path and line.

To act on findings, select one or more items and send them to the Composer.
DCC builds a structured prompt with the selected finding details, including
severity, file, line, comments, and CodeRabbit suggestions when available.

## Troubleshooting

If the panel cannot run a review:

- Confirm the CodeRabbit CLI is installed and available in the desktop app
  environment.
- Reconnect the CLI from `Settings > Account`.
- Run the CLI directly in a terminal to confirm account and network access.
- Check whether the selected workspace has a Git diff for the chosen review
  scope.

Typical failure categories include missing CLI, authentication problems,
permission errors, network failures, rate limits, Git errors, invalid
configuration, timeouts, and empty diffs.
