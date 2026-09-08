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

## Note capture and branch context

The lightbulb is a compact 32 × 32 px balloon with an accessible name and a tooltip
that includes its shortcut. It occupies its own place after the worktree control,
separate from the project and branch group on the right.

The branch has no fixed truncation limit; long names wrap when necessary. Capture
preserves the current text selection and identifies the composer's session.
Outside the conversation, the notes library and Cmd/Ctrl + Shift + N remain available.

## Next increments

1. **Composer attachments and context:** improve review, preview, and removal of
   attached files, images, and pasted context while preserving draft and send behavior.
2. **Settings hierarchy:** improve grouping and navigation inside settings.
3. **Long conversations:** refine the presentation of agent steps and tool history.

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
