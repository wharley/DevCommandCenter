# DCC visual direction

Project notes establish a shared visual language for the desktop: quiet surfaces,
clear hierarchy, restrained depth, and motion that explains an interaction. Each
area adapts these patterns to its purpose.

## Direction by area

| Area | Direction |
| --- | --- |
| Sidebar | Identify the current project and keep its tasks easy to scan. |
| Tasks | Make the objective and state legible, with accessible row actions. |
| Conversation | Prioritize reading and distinguish live activity from tool history. |
| Composer | Make attachments, context, and controls easy to review before sending. |
| Dialogs and settings | Share surface, focus, and motion patterns with the notes library. |
| Diff inspector | Keep comparison scopes visible and branch context readable. |
| Pull Requests | Connect the request list, summary, and code review with clear hierarchy and compact navigation. |
| Skills | Separate the editable library from detected context and make agent targets easy to compare. |
| Open and clone project | Group source, destination, and task setup, with persistent footer actions. |
| Task objective | Separate outcome, completion criteria, limits, and live status while keeping actions visible. |

## Implemented foundation

- Shared radius, surface, shadow, accent, and motion tokens in
  `apps/desktop/src/styles/dcc-design.css`, derived from the light/dark theme.
- Grouped sidebar navigation. The current project uses a stronger heading without
  an enclosing card; task cards are indented beneath it. The virtualized list
  accounts for the spacing between headings and tasks.
- Project shortcuts on the new-task screen, with the full project picker retained.
  Protected/local execution remains explicit; pending creation prevents duplicate clicks.
- Composer depth and focus treatment, and a subdued tint for live agent activity.
- Consistent shared dialog corners, shadows, and transitions. Reduced-motion
  behavior preserves dialog exit lifecycle events.
- Open task notes restore automatically on returning to their source or linked
  implementation task, retaining position and minimized state without taking focus.
- Composer context review: a collapsible summary lists attached files, images,
  and pasted snippets with individual preview and removal actions.
- Settings navigation groups the nine existing sections by purpose, with section
  search, a compact layout, and visual previews for the light/dark theme choices.
- Assistant activity keeps the latest recorded step visible, with chronological
  details, filters, explicit execution states, and incremental history rendering.

## Review surfaces

The diff inspector exposes Workspace, Last turn, and Branch as visible segmented
controls. Each scope has a file count and a short explanation of its comparison.
Missing or still-loading summaries use a dash rather than a misleading zero.
The existing scope selection rules, lazy diff loading, and Git operations remain
in place. File actions in the tree also become visible when the row has keyboard
focus. Workspace and last-turn diff cards share the same surface treatment.

Branch context uses a shared component in the inspector and PR summary. Source
and base names wrap, with more room reserved for the source branch. The inspector
continues to compare committed changes using the existing base-to-HEAD merge-base
comparison; local changes remain in the Workspace scope.

Pull Requests use a subtle selected-card accent, grouped summary sections, and
consistent focus treatment. Filters and review controls expose their selected
state to assistive technology. Below 780 px of available hub width, selecting a
request opens its detail with an explicit back action and keyboard focus transfer.
Below 620 px of detail width, file navigation sits above the diff. Code keeps its
own horizontal scrolling; the review footer remains accessible within its region.
Existing comment, agent-review, merge confirmation, and permission checks remain
in place.

These surfaces extend the original three increments. The scope is visual and
navigational; it does not introduce new forge actions or change Git semantics.

## Code selection and line annotations

Diff and file selections use a shared rounded action button and restrained gutter
plus controls. The styles are also injected into Pierre's shadow roots, including
the editable file surface; its positioning wrapper does not draw a second card.
Native code selection, editing, line ranges, and original/modified side handling
retain their existing behavior.

The shared annotation balloon identifies the file and line range, previews the
selected source, and separates instruction entry from destination actions. Source
previews scroll and display at most 6,000 characters; the complete captured snippet
is retained for submission, composer editing, or evidence collection. A close
button and Escape dismiss the balloon, Tab cycles through its controls, and
Cmd/Ctrl + Enter submits the instruction. The surface uses the shared radius,
accent, shadow, and motion tokens and honors reduced motion.

PR line comments use the same surfaces and focus treatment. Inline composers and
comment cards size themselves against the visible diff region and remain reachable
while code scrolls horizontally. In compact layouts, the overall review footer
temporarily gives its space to the inline composer and returns when the draft is
saved or cancelled, retaining its input. Draft comments stay local until the existing
review submission action is used.

## Open and clone project

Both workspace creation flows share a header, accent, surfaces, field sizing, and
fixed action footer. The form body scrolls independently of the header and footer.
Clone groups repository URL, local destination, and task configuration into separate
cards. Opening a project preserves single/multiple repository selection, branch
resolution, and the protected-worktree preview. The generic entry is titled “Open
project”; repository-specific and multi-project entries retain “New task”.

Long context paths and protection branches wrap instead of clipping. Repository
choices expose full names and native branch choices retain the full value in their
title. Multi-project selection has a visible focus ring for its hidden checkbox.
Compact layouts retain access to actions, and motion respects system preferences.
The native directory picker, creation payloads, validation, setup reporting, and
post-creation composer focus behavior are preserved. Pending creation hides the
close control while the existing dismissal guard remains active.

## Task objective

The objective keeps its composer-anchored popover, with a shared surface, a calm
accent header, larger outcome/acceptance fields, and a dedicated execution-limits
card. Persisted objectives show their status, pause reason, turn and failure
counters, retries, and revision. Automatic-dispatch restrictions remain visible
alongside the status that explains them.

The body scrolls within the available window height while save, transition, and
clear actions remain in a fixed footer. Compact windows stack the limit fields.
The panel has an accessible name, an explicit close action, Escape/focus return,
and reduced-motion support. Loading and read failure have explicit states and a
retry action; unavailable data disables mutations. Pending writes keep the panel
open. Existing validation, generation checks, session API calls, and objective
execution semantics remain in place.

## Skills library and editing

The skills dialog has three sections: the editable library, detected project agent
context, and the bundled DCC catalog. Library search matches names, descriptions, and target agent names. Cards
show readable descriptions, compact target labels, and explicit edit/delete actions.
The orchestration preset lives in the DCC catalog and opens a review form before
anything is saved. “Add to project” creates an editable copy and returns to the
library. Cancelling returns focus to the catalog. Names are fixed when adding a
catalog template; its presence in the library determines the “Added” indicator.
Deleting that copy makes the template available again. Project edits never modify
the bundled template. `skill-catalog.ts` lists the bundled records and translated
card metadata for future additions.

The form gives instructions a larger editor area and presents each target as a
selectable card with its native destination. Existing invocation behavior and the
explanation of always-on targets remain visible. Save actions stay in a fixed
footer while form content scrolls; selected targets and the invocation switch are
keyboard accessible. Inputs and navigation are disabled while a save is pending.
The existing save/delete/compile commands retain their checkout and workspace
arguments, and mutations remain unavailable without a workspace identity.

Loading, load failure with retry, empty search, empty inventory, and no-project
states have distinct presentations. The dialog uses shared surface/accent tokens,
visible focus, and compact layouts for fields, targets, and cards. Detected external
files remain an inventory; opening that section does not import or rewrite them.

## Note capture and branch context

Floating project notes use rounded cards without a pointer, in both expanded and
minimized states: their movable position does not imply a fixed visual anchor.

The lightbulb is a compact 32 × 32 px balloon with an accessible name and a tooltip
that includes its shortcut. It occupies its own place after the worktree control,
separate from the project and branch group on the right.

The branch has no fixed truncation limit; long names wrap when necessary. Capture
preserves the current text selection and identifies the composer's session.
Outside the conversation, the notes library and Cmd/Ctrl + Shift + N remain available.

## Composer context review

The review appears when the editor contains attachment or pasted-snippet badges.
It starts collapsed to preserve writing space. Expanded cards distinguish item
types and show paths to disambiguate equal filenames. Removing an item affects
only that occurrence in the draft and can be undone in the editor.

Previews open on demand in an accessible dialog. Local files are read through
`preview_composer_attachment`; text is rendered as plain text, and PNG, JPEG, GIF,
and WebP are displayed as images. File previews are limited to 1 MiB for text and
8 MiB for images. Missing, unsupported, or oversized files remain attached, with
an explanation when a preview cannot be displayed. Relative paths must resolve
within the workspace; explicit absolute attachment paths are supported.

Drafts with context retain a versioned editor-state companion in local storage,
alongside the existing plain-text draft. This restores badge types after navigation
or reload without changing the text sent to a provider. Clearing or replacing the
draft clears the companion; legacy drafts remain readable as plain text. Image
bytes are not duplicated in draft storage. Temporary image files still depend on
their existing lifetime on disk. Notes inserted as ordinary text remain text.

## Settings hierarchy

The settings sidebar now groups the existing destinations into personal
preferences (general, appearance, shortcuts), agents and services (providers,
integrations, connections, account), and project/advanced options (Git,
experimental). The active destination has a visible selection state; the content
header identifies its group, title, and description. The sidebar footer identifies
the current workspace, while workspace actions retain their existing availability.

Section search matches translated names, descriptions, and keywords without
requiring accents. It filters navigation, not individual controls or account data.
Enter opens the first result; Escape clears a nonempty focused search before
dismissing the dialog. Empty results offer a reset. The existing section content
and action handlers remain in place, and switching sections resets content scroll.

Below 720 px, a grouped native section selector replaces the sidebar navigation.
The content keeps its own scroll area. Appearance groups language, theme, and
density into distinct surfaces; the light/dark choices have illustrative interface
previews with explicit selection states. Existing preference persistence is retained.
The dialog uses shared surface and motion tokens, supports reduced motion, and
returns keyboard focus to its opener when that element still exists.

## Long conversations

The assistant activity summary remains visible when its details are collapsed.
It shows the latest recorded action or update and distinguishes active execution,
waiting for user input, interruption, and historical activity. Explicit turn
completion takes precedence over stale streaming flags on restored annotations.
No synthetic progress percentage or elapsed-work estimate is introduced.

Expanded activity retains source order and stable sequence numbers. Filters show
all entries, tool actions, updates (commentary and reasoning), or failures. Failure
counts remain visible when collapsed and open the failure filter directly, including
errors that occurred before the latest page. A failed tool does not label the entire
turn as failed. Approval, user-input, and native-agent supervision cards remain
outside these filters.

History starts with the latest 20 matching entries. Earlier entries can be revealed
20 at a time. The activity body is unmounted when collapsed, and closed tool and
reasoning disclosures do not mount their output. Inspected entries stay mounted as
new events arrive; that deliberate reading window can grow beyond the initial page.
This limits initial rendering work without changing stored events or transcripts.

Automatic collapse retains a short grace period after execution settles. Manual
expansion, collapse, and inspection take precedence; focused or selected content is
not automatically hidden. A tool with an unfinished annotation on a stopped turn
uses a neutral state instead of a success check. Outputs have bounded scroll areas,
and keyboard focus remains visible. Existing response rendering and conversation
navigation retain their behavior.

The three planned increments (composer context, settings, and long conversations)
are implemented. Packaged native-app verification remains part of release review.

Task exit animations need to respect virtualization and scroll position. Current
row transitions update surfaces and selection without retaining a completed task
solely to run an animation.

## Review criteria

- Short and long branch names remain readable without overlap with note capture.
- Task selection, expansion, rename, completion, and restoration retain their behavior.
- Verify expanded/collapsed navigation, smaller windows, and light/dark themes.
- Preserve visible keyboard focus, accessible action names, and reduced-motion preferences.
- Avoid continuous decorative motion and per-token animation in long responses.
- Preserve semantic colors for Git state, permissions, and errors.
- Keep UI fixtures synthetic; exclude local databases, user transcripts, credentials,
  and personal screenshots from repository assets.

## Validation

The fixture `/tests/notes.html?design` renders real components with synthetic data
and callbacks. `apps/desktop/tests/design-smoke.mjs` exercises navigation, creation,
completion, capture, and footer geometry. `notes-return-smoke.mjs` checks restoration
and position persistence. Fixtures are development entries and are excluded from
production bundles. Browser checks complement validation in the native Tauri app.

The composer fixture `/tests/composer-context.html` uses real editor, persistence,
and review components with synthetic preview IPC. Run
`apps/desktop/tests/composer-context-smoke.mjs` against the desktop dev server
(default `http://127.0.0.1:1432`; override with `DCC_COMPOSER_URL`). It checks
previews and retry, removal and undo, unchanged send text, keyboard actions,
conversation isolation, reload, both themes, and compact dialogs. Screenshots go
to a temporary directory, outside the repository. The script accepts
`DCC_PLAYWRIGHT_MODULE` and `DCC_CHROMIUM_EXECUTABLE` for an existing browser setup.

Validation for this increment: 708 desktop tests, desktop TypeScript checking,
the production Vite build, three native attachment-preview tests, and the composer
browser smoke test passed. Native preview tests cover text/image responses, path
boundaries, directories, binary files, and size limits. Browser IPC is simulated;
an end-to-end check in the packaged native app remains a release check.

For settings, `/tests/settings.html` renders the real dialog with synthetic IPC,
disconnected accounts, and an empty provider catalog. Run
`apps/desktop/tests/settings-smoke.mjs` against the desktop dev server; override
its URL with `DCC_SETTINGS_URL`. It uses the same optional Playwright/browser
environment variables as the composer smoke test. It covers all nine destinations,
search and empty results, Escape and focus restoration, update/shortcut callbacks,
theme/density/language controls, persistence, and compact layout without horizontal
overflow. Both themes and reduced motion are checked. Account authentication and
native integration operations are outside this fixture's scope.

Settings validation: the browser smoke test, 708 desktop tests, TypeScript checking,
and the production Vite build passed. Screenshots remain in a temporary directory
outside the repository.

For activity, `/tests/activity.html` renders the real assistant message with synthetic
events. `apps/desktop/tests/activity-smoke.mjs` accepts `DCC_ACTIVITY_URL` and the
same optional browser environment variables as the other smoke scripts. It checks
1,000-entry history, lazy output mounting, chronological pagination, filters, old
failures, live/collapsed summaries, completion, interruption, pending approval,
native-agent visibility, retained inspection, keyboard operation, both themes, and
compact layout with reduced motion. Provider execution and permissions are not
sent to a live backend by this fixture.

Activity validation: 710 desktop tests, TypeScript checking, the production Vite
build, and the activity browser smoke test passed. The browser fixture complements
native-app release verification; it does not run a real coding-agent session.

For review surfaces, `/tests/review-surfaces.html` renders the actual inspector and
PR hub with synthetic read-only IPC responses. Run
`apps/desktop/tests/review-surfaces-smoke.mjs` against the desktop dev server;
`DCC_REVIEW_URL` and the same optional Playwright/browser environment variables
are supported. Coverage includes all three scopes, rendered workspace/turn/PR
diffs, empty states, list/tree switching, full branch names, PR filtering/search,
summary/code navigation, keyboard focus restoration, both themes, reduced motion,
and widths down to 360 px. The fixture asserts that no mutation IPC is sent.
It does not validate live forge operations or native Git execution.

Review-surface validation: 710 desktop tests, TypeScript checking, production Vite
build, and the browser smoke test passed. Captured screenshots were visually
reviewed and remain outside the repository in a temporary directory.

For code annotations, `/tests/code-annotation.html` renders the actual diff, file,
editable editor, and shared annotation balloon with synthetic content and local
callbacks. Run `apps/desktop/tests/code-annotation-smoke.mjs` using the same browser
environment variables; `DCC_ANNOTATION_URL` overrides the dev-server URL. Coverage
includes original/modified gutter actions, read-only line ranges, editable text
selection, annotations after an edit, all four destination actions, focus cycling,
Escape, limited previews with full submission payloads, both themes, and a compact
viewport. The review-surface smoke test also checks inline PR drafts, removal,
cancellation, and the compact review footer's return without sending mutation IPC.

Code-annotation validation: 710 desktop tests, TypeScript checking, the production
Vite build, and both browser smoke tests passed. Screenshots were visually reviewed
and remain in temporary directories outside the repository. Live agent execution
and publishing forge comments are outside these fixtures' scope.

For skills, `/tests/skills.html` renders the actual dialog with an in-memory IPC
fixture. Run `apps/desktop/tests/skills-smoke.mjs` with the same browser environment
variables; `DCC_SKILLS_URL` overrides the dev-server URL. Coverage includes search,
detected context, catalog navigation, preset copy/add/delete/re-add, create/edit/delete,
target selection, invocation,
validation, load retry, compile failure, read-only/no-project states, keyboard,
both themes, reduced motion, and a 390 px viewport. No real skill files are changed.

Skills validation: 710 desktop tests, TypeScript checking, the production Vite
build, and the browser smoke test passed. Screenshots were visually reviewed and
remain outside the repository. Native compilation of agent files is outside the
browser fixture's scope.

The DCC catalog follow-up passed the 8 focused skills tests, TypeScript checking,
the production build, and the expanded browser smoke test. The browser scenarios
verify cancellation without writes, project-copy customization, the added state,
delete/re-add with original template content, and read-only access.

For objective UI checks, `/tests/objective.html` renders the actual control with
in-memory session IPC. `apps/desktop/tests/objective-smoke.mjs` supports the same
browser environment variables; `DCC_OBJECTIVE_URL` overrides the dev-server URL.
Coverage includes create/edit validation, generation payloads, pause/resume/complete,
clear, read retry, failed-save draft retention, loading, keyboard focus, both themes,
reduced motion, and compact layouts. The fixture never changes a real conversation.

Objective validation: 3 focused logic tests, TypeScript checking, the production
build, and the browser smoke test passed. Screenshots were visually reviewed and
remain in a temporary directory. Native agent execution is outside the fixture.

For open/clone UI checks, `/tests/workspace-dialog.html` renders the actual dialog
with simulated directory selection, branch reads, source resolution, and creation
callbacks. Run `apps/desktop/tests/workspace-dialog-smoke.mjs` with the same browser
environment variables; `DCC_WORKSPACE_DIALOG_URL` overrides the dev-server URL.
Coverage includes single/multiple projects, source branches, clone payloads, folder
selection/cancellation, pending and failed operations, keyboard selection/dismissal,
both themes, reduced motion, and compact layouts with long paths and branches.
No repositories are cloned or modified, and no native directory picker is opened.

Open/clone validation: 12 focused logic tests, TypeScript checking, the production
build, and the browser smoke test passed. Screenshots were visually reviewed and
remain outside the repository. Live Git and native picker behavior are outside the
browser fixture's scope.
