# Agent Workbench Roadmap

Status: proposed public roadmap  
Last updated: 2026-08-27

## Vision

Dev Command Center (DCC) is a local-first, multi-provider workbench for
directing, reviewing, validating, and delivering work performed by coding
agents. Its unit of work is an isolated workspace, not an isolated chat.

The product direction is not to become a general-purpose IDE or another chat
wrapper around provider CLIs. DCC should make the full engineering loop easy
to supervise:

```text
intent -> isolated agent work -> review -> validation -> human decision -> delivery
```

This is a practical distinction. Agents can already make substantial changes;
the difficult human job is keeping parallel work understandable, reversible,
and safe to ship. DCC's local-first architecture, Git worktrees, provider
neutrality, terminal, review surfaces, delegated sessions, and delivery model
already provide the foundation for that job.

## Product positioning

**DCC is the open-source, local-first multi-provider workbench where agent
work remains connected to its workspace, evidence, review, and delivery
decision.**

The roadmap makes that promise more visible and reliable. It does not depend
on selling model tokens, replacing provider CLIs, or collecting repository
content in a hosted service.

## Principles

1. **Workspace first.** A thread, terminal, diff, validation, and delivery
   state must retain their relationship to the relevant repository, worktree,
   branch, and task.
2. **Local first and private by default.** Repository content, prompts, paths,
   session data, and credentials stay local unless the user explicitly enables
   a provider or integration that requires otherwise.
3. **Human decision points are first-class.** The product should make it easy
   to answer: what changed, why, how was it validated, and what can I safely do
   next?
4. **Provider-neutral domain.** Providers are adapters. Workspaces, reviews,
   delivery state, and safety guarantees belong to DCC.
5. **Progressive disclosure.** Expert controls remain available without making
   the daily workbench noisy or difficult to learn.
6. **Evidence before automation.** Automation may prepare a decision, but may
   not silently discard work, force-push, bypass hooks, or merge code.
7. **Small vertical slices.** Each milestone must leave an independently useful
   and testable product improvement; large rewrites are not a delivery method.

## Non-goals

- Replacing VS Code, JetBrains, GitHub, GitLab, or CI platforms.
- Rebuilding every existing surface as a multi-pane layout in one release.
- Making an undo button that is a hidden `git reset`, destructive checkout, or
  force-push.
- Introducing a cloud account requirement, a token resale model, or default
  collection of workspace content.
- Copying another product's visual identity or provider-specific terminology.
- Opening release signing, credential handling, or destructive Git workflows
  to unreviewed changes.

## Milestones

The sequence intentionally starts with trust and visible workflow value. Status
labels show planning state, not a delivery date.

| Milestone | Current status | Deliverables | Acceptance criteria |
| --- | --- | --- | --- |
| **M0 — Trust and measurement baseline** | Implemented locally and review-approved — pending deploy | Publish the product narrative and a small, opt-in, local-only measurement model. Document the audited download-statistic methodology and limitations, and make the landing/README show the canonical workflow. | Every public metric has a source, definition, and update date. No public number is described as unique people unless it is actually deduplicated. Local product signals exclude prompt content, repository paths, session IDs, and credentials; users can inspect/reset them. |
| **M1 — Workspace Split View v1** | Committed locally (`9ee3625`) — pending release | Keep conversation as the fixed primary pane and allow exactly one secondary surface: Changes, Terminal, Files/Editor, or Preview. Add resizing, persistence per project, keyboard focus, and responsive fallback. This is a frontend layout milestone: no new backend orchestration model. | Conversation plus each secondary surface works without losing session state. A compact layout remains usable. The selected surface and ratio restore safely. Keyboard navigation and focus restoration work. Existing terminal and Inspector workflows do not regress. |
| **M2 — Unified Palette** | Committed locally (`9ee3625`) — pending release | Deliver the first palette slice: `Cmd/Ctrl+K` alias, local debounced project/session/file discovery, explicit `@` session FTS search, and a capped recent-items list (40). Continue evolving it toward projects, workspaces, threads, and contextual actions such as last diff, terminal, and active review. | The v1 results navigate to the correct entity and respect project/workspace context. Search is local, session FTS is explicit through `@` plus at least two local characters and debounced, and recent items are capped at 40. Prompt content is not indexed by default. Commands show a clear target before mutation. |
| **M3 — Last Turn Review** | Committed locally (`754d751`) — pending release | Capture a bounded, attributable change snapshot for a completed agent turn and expose `Changes from last turn` in Split View, the Inspector, and review cards. Snapshots preserve base/result evidence, changed-file manifests, immutable historical previews, relevant validation evidence, and a clear distinction from accumulated workspace changes. | A user reaches the last-turn diff in one action. DCC does not attribute pre-existing or subsequent manual changes to the agent turn. No-change, unavailable Git data, compatibility, and failed collection are distinct states. Relevant tests cover normal, concurrent, multi-root, and changed-workspace cases. |
| **M4 — Guarded Undo** | Phase 0 implemented and approved (`f1cded2`); Phase 1 in progress | Implement the strict capture v2 and fingerprint-guarded restoration contract in the [Guarded Undo design](GUARDED_UNDO_DESIGN.md). Phase 1 is capture-only (behind a feature flag), with a physical-root coordinator, active turn intervals, private raw preimages, retention, and startup recovery; the Undo button remains disabled. | Capture v1, Git trees, and snapshot quarantine remain NO-GO restoration sources. Known overlapping DCC turns/mutations on one physical root make capture ineligible. Capture failure never blocks terminal turn events. A single-instance app-data lock gates startup/retention, and Windows remains adapter-unsupported until handle-relative tests pass. |
| **M5 — Release-grade macOS distribution** | Implemented locally and statically reviewed — pending macOS CI with Apple signing/notarization secrets | Add signed, notarized, stapled DMGs for supported macOS architectures while retaining the existing signed `.app.tar.gz` updater path during a verified migration. Publish checksums and installation guidance. | A clean macOS installation works from the DMG without avoidable Gatekeeper warnings. Updates preserve data. DMG, updater archive, architecture selection, checksums, and fallback behavior are validated in release checks. Release secrets stay isolated. |
| **M6 — Delivery integration** | Proposed | Connect turn review, validation evidence, and safe recovery to the existing Delivery Status / delivery-workflow model. Add only reviewable automations that land in a queue. | A workspace can answer what changed, what was validated, what blocks delivery, and which human action is next. Delivery actions revalidate captured branch, workspace, remote, and push target before mutation. Automations never merge, force-push, or discard work silently. |

## Current implementation status

The following work is present locally and has passed static or product review;
none of it is marked shipped.

- **M0:** the landing metric correction and non-blocking GitHub-star CTA are
  implemented locally and review-approved, pending deployment.
- **M1:** Split View v1 is committed locally in `9ee3625`, pending release.
- **M2:** Unified Palette v1 is committed locally in `9ee3625`, pending
  release. Its session FTS remains local and is only invoked by
  an explicit `@` query with at least two local characters; searches are
  debounced, `Cmd/Ctrl+K` is an alias, and recents are capped at 40.
- **M3:** Last Turn Review is committed locally in `754d751`, pending release.
  See the implementation scope and deliberate limitations below.
- **M4:** Phase 0 schema/fixtures are implemented and approved in `f1cded2`;
  Phase 1 now has cancellation-safe single-root START/terminal integration,
  physical M3→M4 binding, process-scoped recovery, and the app-data lifetime
  lock behind a default-off feature flag. Workspace commands now share the
  capture runtime and authorize mutation roots against the durable SQLite
  mapping. Editor writes, automation/spec writes, conflict actions, and basic
  stage/unstage/discard actions participate in the physical-root coordinator;
  atomic multi-root mutation admission is implemented for the later
  delegation/removal slice. Capture v1 remains review evidence only and is a
  NO-GO for Undo. Until setup/tasks, commit/push/sync/delivery,
  delegation/worktree creation, implicit generated-context writes, and
  workspace/repository removal are also covered, completed captures finalize
  explicitly ineligible and cannot create an `Eligible` restore set. Recovery
  is lazy but gates the first feature-enabled begin. The Undo button remains
  disabled.
- **M5:** DMG support for Apple Silicon and Intel, while retaining the updater
  archive path, is implemented locally and static-review approved. It remains
  pending macOS CI that has the required Apple signing and notarization secrets.

The current validation baseline is 95 frontend test files / 511 tests, 83
feature-off and 163 feature-on `dcc-infra` Rust tests, and 194 `dcc-tauri`
Rust tests in either feature mode (6 ignored).

### M3 Last Turn Review local implementation

The local implementation captures per-turn, per-root snapshots in a dedicated
table, outside transcript storage and session FTS. Historical content is read
through an immutable `snapshotId` plus path preview; it is not reconstructed
from the current workspace. Base and result evidence are captured before the
terminal phase, and the lifecycle covers desktop, queue, HTTP, and abort paths.

It includes explicit state reporting, compatibility handling, and multi-root
coverage. The review is reachable from Split View, the Inspector, and the
conversation/review card surfaces, with English and Brazilian Portuguese copy.

The following limitations are deliberate in v1:

- Untracked files record manifest evidence only; their content and historical
  preview are unavailable.
- Compatibility cannot be inferred for older data; affected snapshots are
  explicitly unavailable rather than guessed.
- Repositories with clean filters or process filters, including Git LFS, are
  unavailable for snapshot capture in this version.
- Diff generation, manifests, and capture deadlines are bounded. Any bound or
  collection failure is reported as an explicit state.
- Temporary snapshot quarantine is isolated from the workspace and swept on
  startup. It is capture infrastructure only, not a source of user-visible
  restoration.
- A Phase 1 sentinel filter test must prove that no M4 capture or future
  restoration path accepts `captureVersion = 1`, M3 Git trees, rendered diffs,
  or quarantine objects as restoration content.

## Dependency and risk map

```text
M0 trust/measurement
        |
        +--> M1 split view ----> M2 unified palette
        |
        +--> M3 turn snapshots --> M4 guarded undo --> M6 delivery integration
        |
        +--> M5 release-grade DMG
```

M1 deliberately precedes snapshots: it makes the existing conversation,
terminal, changes, editor, and preview surfaces feel like one workbench without
introducing a new data model. M3 establishes review provenance locally. M4 now
has Phase 0 implemented and approved (`f1cded2`) and Phase 1 in progress under
the [safe restoration design](GUARDED_UNDO_DESIGN.md). Capture v1, Git trees,
and M3's isolated quarantine are not restoration sources. M5 may progress
alongside the product work, but its signing and release controls remain
independently gated.

### Guardrails

- Treat Git state captured for review or restoration as evidence with a bounded
  lifetime, not as permanent truth. Revalidate before every mutation.
- Snapshot metadata must be versioned and migration-tested. Old workspaces and
  sessions remain readable even when a snapshot is unavailable.
- Bound stored diff and command output sizes; report truncation clearly.
- Do not capture untracked-file content in M3 v1, and mark clean/process-filter
  repositories (including Git LFS) unavailable rather than risking an invalid
  snapshot.
- Keep snapshot quarantine isolated and sweep it at startup; it is never an
  Undo source.
- Guarded Undo startup cleanup, retention, and recovery require the single
  instance app-data lifetime lock; if it is unavailable, fail closed without
  cleanup.
- Phase 1 captures run off the async/UI runtime and a capture failure must not
  suppress a terminal turn event. A same-user process that can alter both the
  database and artifact directory is outside the integrity boundary; do not
  market these checks as protection against a malicious agent.
- Preserve the distinction between the comparison base, checkout reference, and
  push target. Never infer a write target only from the name `origin`.
- Keep raw runtime diagnostics behind an explicit details affordance. Daily UI
  should show semantic state and the next decision.
- New telemetry is opt-in, local-only by default, content-free, inspectable,
  resettable, and removable without affecting product use.
- Every release artifact needs a reproducible verification path. Never put
  signing keys, notarization credentials, or release endpoint authority in a
  community-contributed workflow without core review.

## Ownership model

### Core-maintained and required-review areas

- Workspace/worktree persistence and SQLite migrations.
- Turn snapshot schema, attribution logic, fingerprints, reconciliation, and
  all undo mutations.
- Git operations that fetch, synchronize, push, delete, apply, or discard.
- Credential storage, sandboxing, local pairing, permission boundaries, and
  privacy/telemetry implementation.
- Provider-runtime contracts that can corrupt sessions or change execution
  permissions.
- Signing, notarization, updater endpoints, release automation, and secrets.

### Community-friendly areas

- Split-view components, responsive behavior, accessibility, keyboard support,
  tests, themes, and localization under an agreed UI contract.
- Documentation, installation checks, examples, project templates, and
  provider setup guides.
- Non-privileged palette commands, result renderers, and search UX.
- Provider adapters after a conformance fixture and owner review are available.
- Release QA scripts that do not contain credentials, plus package-manager
  metadata maintained through documented ownership.

Large contributions should begin with a Discussion or short RFC. Maintainers
will reserve `good first issue` for issues that are reproducible, bounded, and
have an explicit validation route.

## Honest measurement

The release-download audit is complete. The previously displayed total of
**2,988** was the sum of downloads across **386 release assets**; it was not a
count of people or unique installations. In that total, **2,837** downloads
were for `latest.json`, the updater metadata file, while downloadable app
packages accounted for **129** downloads. Release-asset counts can still include
repeated downloads, one user downloading several assets, automated systems, and
updates; even the package-only count is not a unique-user metric.

The landing is being corrected to count and label app packages honestly, with a
non-blocking GitHub-star CTA. Public download reporting must state the exact
asset scope, aggregation period, source, and update date. It must never call an
aggregate download total “people” unless a documented, privacy-preserving
deduplication method actually supports that claim.

The measurement hierarchy is:

1. **Acquisition:** landing visits, source/UTM, GitHub click-through, and
   release-asset downloads by platform.
2. **Activation:** first workspace, first provider session, and first review of
   a Last Turn change set.
3. **Workflow value:** terminal opened from workbench context, validation run,
   feedback sent, review decision made, and delivery state reached.
4. **Reliability:** snapshot-collection failures, undo completion, undo
   fingerprint conflicts, release-install success, and delivery recovery by
   failure class.
5. **Retention:** return activity after 7 and 28 days, calculated locally and
   shared only as anonymous aggregate opt-in data if enabled.
6. **Open-source health:** GitHub visitors/cloners, stars, issues with a
   reproduction, first PRs, merged PRs, and maintainer first-response time.

Stars are a trust and discoverability signal, not the product north-star. The
practical conversion path is:

```text
visitor -> verified download -> first workspace -> first review/value moment
        -> GitHub visit -> star, feedback, issue, or contribution
```

The product may offer a non-blocking GitHub-star invitation after a user has
received clear value. It must never gate features, updates, or support behind a
star.

## First technical slice: M1 Workspace Split View v1

**Current implementation state:** committed locally in `9ee3625`, pending
release. It is not marked shipped.

### Outcome

Make the current workbench feel coherent before adding new stateful features.
The user keeps the active conversation visible while opening one existing work
surface beside it.

### Scope

- Fixed primary conversation pane.
- One secondary slot with `changes`, `terminal`, `files`, `editor`, or
  `preview` when that surface is available for the active workspace.
- Resize handle, minimum readable dimensions, and an explicit return to the
  existing single-pane/default layout.
- Per-project local layout preference: selected secondary surface and ratio.
- Desktop keyboard shortcuts, focus movement, focus restoration, ARIA labels,
  and narrow-window fallback.
- Reuse existing surface ownership, queries, and commands. No duplicate
  terminal session, review cache, backend event stream, or workspace state.

### Explicitly out of scope

- Arbitrary grid layouts, detached windows, drag-and-drop pane trees, and more
  than one secondary pane.
- A new backend layout model, new session protocol, or data migration beyond a
  small versioned UI preference if one is required.
- Last-turn snapshots, undo, global indexing, or delivery behavior changes.

### Suggested implementation sequence

1. Map the current workbench shell and surface mounting points; write a small
   UI contract identifying which component owns each surface and its lifecycle.
2. Add a typed local layout preference with safe defaults and validation of
   stale/unknown surfaces.
3. Implement the two-column shell with fixed conversation priority, minimum
   sizes, and a single secondary-slot switcher.
4. Move or adapt existing Changes/Terminal/Files/Editor/Preview rendering into
   that slot without changing their data ownership.
5. Add responsive fallback and focus/keyboard behavior.
6. Add component and end-to-end tests for state preservation, persistence,
   accessibility, and compact widths.
7. Instrument only local, content-free interaction timing already permitted by
   the UX measurement policy; provide inspect/reset controls.

### M1 exit checklist

- Opening and closing the secondary slot does not lose composer text, active
  conversation state, terminal tabs, or Inspector selection.
- The terminal still observes its existing scope and lifecycle rules.
- All secondary surfaces degrade to the existing single-pane path when space or
  availability is insufficient.
- No backend API, workspace schema, or provider protocol is required for v1.
- English and Brazilian Portuguese copy remain complete for touched controls.
- Desktop type checks and focused UI tests pass; manual QA covers a narrow
  window and a restored project layout.

## Related documents

- [UI/UX modernization](UI_UX_MODERNIZATION.md): current workbench principles,
  terminal/Inspector behavior, shortcuts, and local UX measurement rules.
- [Delivery workflows roadmap](DELIVERY_WORKFLOWS_ROADMAP.md): fork-safe
  workspaces, delivery state, review/pipeline integration, and conservative
  recovery requirements.
- [Delegation agents](DELEGATION_AGENTS.md): existing child-session and
  isolated-worktree workflow.
- [Terminal scope and integration plan](PLANO_TERMINAL_INTEGRADO_E_ESCOPO.md):
  terminal behavior that Split View must preserve.
- [Monaco Editor in Tauri](MONACO_TAURI.md): editor implementation constraints.
- [Guarded Undo](GUARDED_UNDO_DESIGN.md): capture v2 eligibility, prepare and
  execute contracts, mutation coordination, journaling, and recovery.
- [Open-source release checklist](OPEN_SOURCE_RELEASE_CHECKLIST.md) and
  [release guide](RELEASING.md): distribution and release controls.
- [Mobile web companion](MOBILE_WEB.md) and
  [mobile pairing security model](SECURITY_MOBILE_PAIRING.md): companion scope
  and trust boundaries.

## Open-source workflow

Milestones are public planning containers. Issues should carry one area label,
one type label, a readiness label, and a concrete acceptance checklist. Use
Discussions for product ideas and RFCs; use issues for reproducible bugs or
bounded, reviewable proposals.

Recommended label groups:

- Areas: `area/workbench`, `area/review`, `area/git-delivery`,
  `area/providers`, `area/search`, `area/release`, `area/docs`, `area/mobile`.
- Types: `type/bug`, `type/feature`, `type/docs`, `type/test`, `type/rfc`.
- Readiness: `status/needs-triage`, `status/needs-design`, `status/blocked`,
  `status/ready`.
- Community: `good first issue`, `help wanted`, `mentor available`.
- Risk: `risk/data-safety`, `risk/security`, `risk/migration`,
  `breaking-change`.

This keeps the roadmap usable by maintainers and contributors without turning
the issue tracker into an unprioritized feature list.
