# DCC Feedback

The feedback icon beside Settings opens **New feedback** and **My feedback** in the desktop app. It remains accessible with the sidebar collapsed.

Connect a GitHub account through **Settings → Accounts** (GitHub CLI must be installed). Feedback uses the account selected in DCC for `github.com`. Its destination is always `wharley/DevCommandCenter`, regardless of the current workspace or its remote.

## Submit and follow a report

1. Choose Bug, Improvement or Other and write a title and description. Bugs also support optional reproduction steps and expected behavior.
2. Choose whether to include the DCC version, operating system and architecture. Conversations, files, repository paths and logs are never collected by this feature.
3. Review the report, then choose **Publish issue**. Publication is public and attributed to your GitHub account.
4. Follow the returned issue in **My feedback**, or open it on GitHub to read and participate in the conversation.

The list includes issues authored by the selected account in the DCC repository, including reports submitted on GitHub. It excludes pull requests and provides pagination. Refresh fetches current states; a failed refresh retains the account's cached results with an explicit notice. Logging into the same account on another installation recovers reports from GitHub.

Open, Completed, Closed without implementation and Closed reflect GitHub's state and closure reason. An issue closed without a reason is not presented as a completed fix. Categories are recorded in the title/body; submission does not require permission to set repository labels.

## Drafts and interrupted submissions

Drafts and per-account history pages are stored locally in the desktop webview. A SQLite receipt is reserved before each publication. A repeated request reuses its receipt instead of posting again. After a timeout or lost response, **Check submission** looks for the existing issue; it never repeats the uncertain POST. Recovery checks up to 300 recent items authored by the account. If no match is found, inspect GitHub before choosing **Start another report**.

Only definite GitHub client rejections allow a corrected request to be submitted again with the same request ID. No background process publishes drafts. This feature requires no DCC-hosted service or embedded GitHub credentials.

## Validation

- `cargo test -p dcc-tauri commands::feedback::tests --lib` covers validation, fixed destination URLs, closure reasons, durable receipts, lost-response recovery and definite rejection retries.
- `yarn workspace @dcc/desktop test src/features/feedback/feedback-api.test.ts` covers local draft recovery, account/page cache isolation and state mapping.
- Serve the desktop Vite app and run `node apps/desktop/tests/feedback-smoke.mjs` against `/tests/feedback.html`. Set `DCC_FEEDBACK_URL`, `DCC_PLAYWRIGHT_MODULE` and `DCC_CHROMIUM_EXECUTABLE` when needed. This isolated fixture uses synthetic IPC; it never authenticates or creates real GitHub issues. Screenshots default to `/tmp/dcc-feedback-smoke`.
