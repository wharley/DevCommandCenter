# Project notes

Project notes capture ideas without adding them to an agent's active scope. Open
**Notes** in the sidebar to search by text, filter by project or conversation of
origin, and switch between open and completed notes.

## Capture and work with a note

- Click the lightbulb after the worktree control in the composer footer, or press
  **Cmd/Ctrl + Shift + N**. The compact balloon has its own space in the footer;
  its tooltip shows **Capture idea** and the keyboard shortcut.
  Selected text is copied into the note's preserved context. Without a selection,
  the note starts empty.
- A completed assistant reply has a note action alongside its copy/fork actions.
  It captures the selected excerpt, or the reply text when nothing is selected.
- Edit the title and body in the floating balloon. Changes save automatically;
  the footer distinguishes pending writes, successful saves and failures.
- Drag the balloon by its header, or focus the handle and use arrow keys
  (Shift moves farther). Collapse it to a bubble, choose a color, or pin it
  across project changes. Up to four balloons can be open. Closing a balloon
  keeps the note in the library and hides it for the current visit.
- Returning to a task automatically restores its open notes, including notes linked
  through **Create task**. Position and minimized state are preserved across task
  changes and app reloads. Automatic restoration does not take keyboard focus.
  Unpinned notes from other tasks leave the screen; pinned open notes remain.
  Completed and deleted notes never reopen automatically. Up to four balloons
  restore at once; the library retains access to the rest.
- **Add to current conversation** appends a reviewable composer draft. It does
  not send a message or start an agent.
- **Create task** creates a protected worktree in the note's registered project
  and prepares its composer with the note and preserved context. Review and
  send explicitly. The note records the implementation task so a repeated click
  does not create another task while that link exists.

## Completion and deletion

Mark a note complete to remove it from the default open list. Completed notes
can be reopened or permanently deleted. The completed filter offers bulk
deletion for the currently filtered notes, with confirmation.

Completing an implementation task offers to complete its linked open notes.
The user can keep them open when work remains. Deleting the task never completes
or deletes its notes.

Notes live in `dcc_project_notes` in the existing application SQLite database,
outside worktrees. Each note owns its title, body, saved context, project label,
and source title. Session/workspace/implementation links use `ON DELETE SET NULL`.
Deleting a source conversation, task, or repository therefore preserves the
note's content. Notes from a removed project remain accessible under **All
projects**; reopen/register the project to create implementation tasks there.

The body is limited to 20,000 characters and context to 12,000 characters. Context
is an explicit text snapshot, not a retained transcript, dependency tree or Undo
capture. Larger context selections are rejected with an explanation. The
frontend keeps bounded pending text drafts for recovery in local storage; SQLite
remains authoritative. Saves serialize per note and use optimistic revisions to
prevent delayed writes from overwriting newer edits.

## Verification

- `cargo test -p dcc-infra notes::tests --lib`: real SQLite workspace-history
  deletion, database reopen, completion/reopen, stale-write rejection and limits.
- `yarn workspace @dcc/desktop test src/features/notes/notes-store.test.ts`:
  delayed and failed saves, recovery, deletion and stale library refreshes.
- `apps/desktop/tests/notes-return-smoke.mjs`: task return, minimized position,
  delayed initial loading, temporary dismissal, pinning, linked tasks, completion
  and keyboard focus, using `/tests/notes.html?restore`.
- `yarn workspace @dcc/desktop typecheck` and `yarn workspace @dcc/desktop build`.
- `apps/desktop/tests/notes-smoke.mjs`: isolated browser fixture using the actual
  notes components and IPC client, with synthetic storage and task callbacks.
  Covers themes, filters, drag/keyboard positioning, minimize, pinning, context
  capture, composer drafts, failures, task linking, explicit completion, reopening,
  reload, bulk deletion and viewport bounds. It never starts a real provider or
  reads a user's DCC database.

For browser QA, run the desktop Vite server on port 1432 and execute the smoke
script with Node 22. `DCC_PLAYWRIGHT_MODULE`, `DCC_CHROMIUM_EXECUTABLE`,
`DCC_NOTES_URL`, and `DCC_NOTES_ARTIFACTS` can override local test dependencies,
URL and screenshot directory. The fixture at `/tests/notes.html` is a development
entry and is not part of the production build.

Animations respect reduced-motion preferences. Balloon occlusion participates
in the desktop's existing native-browser overlay handling. Native Tauri launch
and interactions with a real agent are separate from the mocked browser smoke.
