# Roadmap: Delivery Workflows

## Goal

Evolve DCC from isolated workspaces into an assisted delivery workflow without
turning the product into a generic IDE or a replacement for GitHub, GitLab, or
CI platforms.

The intended result is one continuous path:

```text
PR / MR / branch
        ↓
isolated workspace
        ↓
implementation and review
        ↓
feedback and failure recovery
        ↓
push to the correct repository
        ↓
ready to deliver
```

## Principles

- **Workspace-first:** every operation remains associated with its workspace
  and worktree.
- **Safe by default:** fetch, push, and comparison targets must never be
  inferred from the `origin` name alone.
- **Progressive disclosure:** the Inspector shows a calm summary first and
  expands logs, checks, and actions only when needed.
- **Provider-neutral domain:** GitHub and GitLab are adapters; delivery and
  recovery states belong to the DCC domain.
- **No destructive automation:** assisted recovery never bypasses hooks,
  force-pushes automatically, or merges without explicit intent.
- **Data before presentation:** the Delivery Gate is built only after source,
  push, reviews, and pipelines are modeled.

## Technical identity

The concepts in this roadmap belong to the DCC domain and follow its Rust/Tauri
architecture, commands, workspaces, and Inspector. Product copy and names for
types, actions, and components must use DCC terminology rather than names
borrowed from products considered during market research.

## Phase 1 — Fork-safe workspaces

### Deliverable 1.1 — Source and push model

- Persist a `pushTarget` for every imported workspace:
  - remote;
  - branch;
  - repository URL when it differs from the base repository;
  - whether DCC created the remote.
- Keep read compatibility with workspaces persisted before this field existed.
- Maintain a semantic distinction between:
  - the repository and branch used as the comparison base;
  - the reference used for checkout;
  - the repository, remote, and branch used for push.

### Deliverable 1.2 — Fork imports

- Resolve GitHub PRs and GitLab MRs whose head belongs to another repository.
- Reuse an existing remote when it already points to the fork.
- Create a deterministic, collision-free remote name when required.
- Fetch the fork branch and verify the SHA returned by the forge before
  creating the worktree.
- Always push through `pushTarget`, never through an implicit `origin`.
- Indicate in the creation dialog when the source is a fork.

### Deliverable 1.3 — Remote lifecycle

- Record whether DCC created the remote.
- When deleting the last workspace that uses that remote:
  - verify that no other branch or workspace still depends on it;
  - confirm that the URL still matches the expected value;
  - remove only the remote created by DCC.
- Never remove `origin`, `upstream`, or a user-created remote.

### Acceptance criteria

- A fork PR/MR can be opened as an editable workspace.
- Fetch and push cannot accidentally target the base repository.
- A remote changed after workspace creation is detected before push.
- Existing workspaces continue to work without manual migration.
- Tests cover same-repository sources, forks, remote reuse, collisions, and
  cleanup.

## Phase 2 — GitLab delivery in the Inspector

### Deliverable 2.1 — Reviews

- List MR discussions and inline comments.
- Normalize resolved, unresolved, and outdated threads.
- Allow selected feedback to be sent to the agent.

### Deliverable 2.2 — Pipelines and jobs

- Show the pipeline associated with the workspace SHA.
- List jobs, status, duration, and URL.
- Load job logs on demand with a strict size limit.
- Allow retry only when GitLab exposes the action as available.

### Deliverable 2.3 — Review state

- Reviewers and pending approvals.
- Mergeability, conflicts, and whether the branch is behind its base.
- A normalized state shared with GitHub.

### Acceptance criteria

- The main blocker can be identified in the Inspector without opening GitLab.
- Large logs are never loaded or sent to the agent automatically.
- Authentication and rate-limit errors are distinguishable from pipeline
  failures.

## Phase 3 — Assisted recovery

### Deliverable 3.1 — Failure model

Capture a snapshot when an operation fails:

- workspace and branch;
- operation (`fetch`, `pull`, `push`, or `pipeline`);
- remote and push target;
- sanitized and bounded output;
- relevant changed files;
- timestamp and attempt token.

### Deliverable 3.2 — Conservative classification

- authentication;
- non-fast-forward;
- protected branch;
- local hook or lint;
- conflict or divergence;
- transport;
- pipeline or job.

Unknown errors remain ordinary errors and do not receive a misleading AI
action.

### Deliverable 3.3 — Actions

- View details.
- Try again.
- Update or synchronize the branch when safe.
- Send bounded context to the agent.
- Open the external provider.

Actions are derived by the backend from the captured operation and
classification. Before execution, DCC revalidates the workspace, branch, commit,
attempt token, remote, and push target. Mutating actions require explicit user
confirmation; synchronization uses the captured remote branch, stops for manual
conflict resolution, and does not change a fork workspace's review base.

### Acceptance criteria

- Recovery uses the state captured at failure time rather than accidentally
  using the current state.
- Switching branch or workspace invalidates stale suggestions.
- No action uses `--no-verify`, force-pushes, or merges automatically.

## Phase 4 — Delivery Status

Suggested product name: **Delivery Status**. `Delivery Gate` remains the
architectural name.

### Model

The state is derived rather than persisted as truth:

- `in_development`;
- `needs_attention`;
- `blocked`;
- `awaiting_review`;
- `ready_to_deliver`;
- `delivered`.

Possible signals:

- local changes and commits;
- branch divergence;
- conflicts;
- push target;
- PR/MR and mergeability;
- reviews and approvals;
- CodeRabbit;
- pipeline and jobs;
- validations configured for the project.

### UX

- One summary at the top of the Inspector Git section.
- Only the most important blocker or next action is emphasized.
- Details and secondary signals remain behind expansion.
- Rules are adjustable per project; DCC does not invent mandatory policies.

### Deliverable 4.1 — Core derived status

The existing workspace recap is now the Delivery Status projection. It derives
the six states from local Git, active agent work, captured delivery failures,
CodeRabbit findings, normalized reviews and approvals, base divergence, and
pipeline state. It reuses the existing query cache and does not persist or
re-fetch an independent aggregate.

The summary exposes one reason and links its recommendation to an existing Git,
review, pipeline, synchronization, or recovery action. Loading and failed
external queries are represented as unavailable information rather than a
successful gate.

### Deliverable 4.2 — Project policy and secondary details

Projects can now opt into delivery rules through the existing `.dcc.toml`
configuration:

```toml
[delivery]
minimum_approvals = 2
require_pipeline = true
require_resolved_discussions = true
require_current_base = true
require_before_merge_checks = true
```

Every field is optional and defaults to a neutral value. The policy is only
evaluated as a delivery gate after a PR/MR exists, so it never prevents the
workflow from reaching review. DCC preserves provider-reported blockers even
when no project policy is configured.

The compact Delivery Status expansion exposes local Git, captured recovery,
agent review, provider review, pipeline, and before-merge check signals. It
marks project-configured rules without duplicating the detailed Inspector
sections or issuing additional provider requests.

### Acceptance criteria

- The summary explains why a workspace is or is not ready.
- Every recommendation points to an existing action.
- Missing external integration is shown as unavailable information, not as an
  approval.

## Out of scope

- Embedded Chromium browser or Design Mode.
- A terminal with unlimited splits.
- Native mobile applications.
- Replacing GitHub Issues, GitLab Issues, Linear, or CI/CD platforms.
- Remote execution over SSH; it remains a separate architectural initiative.

## Implementation order

1. Persistent `pushTarget` model.
2. GitHub forks.
3. GitLab forks.
4. Safe remote cleanup.
5. GitLab reviews.
6. GitLab pipelines and jobs.
7. Normalized review state.
8. Bounded failure snapshots.
9. Conservative failure classification.
10. Recovery actions.
11. Derived state and Delivery Status UX.
12. Project policy and secondary Delivery Status signals.
13. Opt-in authenticated delivery smoke harness.

## Progress

Updated July 24, 2026:

- [x] Persistent `pushTarget` model with backward-compatible reads.
- [x] Fork-safe imports for GitHub PRs.
- [x] Fork-safe imports for GitLab MRs.
- [x] Reuse, collision handling, and safe cleanup for DCC-created remotes.
- [x] Fork indication in the workspace creation dialog.
- [x] GitLab discussions and inline comments normalized in the Inspector.
- [x] Review threads shown as open, resolved, or outdated.
- [x] Actions to send a review thread to the agent or Composer.
- [x] GitLab pipeline selected by the current SHA and summarized in the
  Inspector.
- [x] Jobs with status, duration, stage, and external links.
- [x] Job logs loaded on demand, sanitized, and limited to 256 KiB.
- [x] Conservative retry for completed, non-archived jobs.
- [x] Provider-neutral review state shared by GitHub and GitLab.
- [x] Reviewers, pending approvals, mergeability, conflicts, and base divergence
  summarized in the Inspector.
- [x] Bounded, in-memory failure snapshots for fetch, update, push, and
  pipeline operations.
- [x] Failure context tied to workspace, branch, commit, remote, push target,
  changed paths, timestamp, and attempt token.
- [x] Credential redaction, terminal sanitization, and conservative size
  limits before failure output reaches the Inspector.
- [x] Provider-neutral failure classification with explicit precedence for
  authentication, non-fast-forward, protected branches, local checks,
  conflicts, transport, and pipelines.
- [x] Ambiguous failures remain `unknown` and are presented without a
  misleading recovery claim.
- [ ] Authenticated smoke test with a real fork on GitHub and GitLab.
- [x] Authenticated, read-only smoke test with a real GitLab pipeline.
- [ ] Log and retry smoke test with a real pipeline that contains jobs.
- [ ] Authenticated smoke test with a real open PR/MR that has review rules.
- [x] Failure snapshots.
- [x] Conservative failure classification.
- [x] Recovery actions.
- [x] Core six-state Delivery Status derivation and compact Inspector summary.
- [x] Failure, review, approval, divergence, and pipeline precedence with
  existing-action routing.
- [x] Optional project delivery policy persisted in `.dcc.toml` with neutral,
  provider-independent defaults.
- [x] Secondary Delivery Status signals available through compact expansion.
- [x] Project-specific delivery policy and secondary signal details.
- [x] Delivery Status.
- [x] Opt-in authenticated smoke harness for GitHub forks and GitLab forks,
  review rules, pipeline jobs, bounded logs, and explicitly confirmed retry.
- [x] Authenticated, read-only GitLab MR adapter smoke test.

Next slice: authenticated fork, job-log/retry, and review-rule smoke tests when
real cases are available. The commands and safety requirements are documented
in [Authenticated Delivery Smoke Tests](AUTHENTICATED_DELIVERY_SMOKE_TESTS.md);
no external state should be created solely for testing.
