# Monaco Editor in the Tauri Desktop Shell

Dev Command Center embeds [Monaco Editor](https://github.com/microsoft/monaco-editor)
in the Tauri 2 webview for:

- the **Code** dock (editable file tabs)
- the **git diff** surface in the workspace panel (read-only diff review)

This document explains why a small amount of shell-specific configuration exists,
what is standard Monaco behavior, and what contributors should avoid changing
without understanding the Tauri constraints.

## Summary

| Layer | Approach |
| --- | --- |
| Text input | Official Monaco option: `editContext: false` |
| Arrow keys / Tab / indent | Official Monaco API: `editor.addCommand()` calling built-in commands via `editor.trigger()` |
| Multi-tab editing | One shared Monaco instance; file swap via model content + language update |
| Git diff | Same Monaco options and navigation commands on both diff sides |

The desktop app does **not** reimplement cursor movement, indentation, or editing
logic. It configures Monaco and, where Tauri's webview breaks default keybinding
context, registers explicit commands that delegate to Monaco's own command
handlers.

## Why Tauri needs extra configuration

### EditContext breaks keyboard input

Monaco 0.53+ can use the browser [EditContext
API](https://developer.mozilla.org/en-US/docs/Web/API/EditContext) for text
input. In the Tauri webview on macOS, that path was observed to break basic
keyboard behavior (arrow keys and Tab did not move the cursor or indent even
when the hidden textarea had focus).

DCC disables EditContext and keeps Monaco's classic textarea input path:

```ts
editContext: false,
```

This is a **documented editor option**, not a custom input layer. The setting
applies to both standalone code editors and diff editors in
`apps/desktop/src/lib/monaco-runtime.ts`.

### Default keybindings may not fire

Monaco's built-in keybindings rely on internal **context keys** (for example,
whether the editor text input is focused). In the Tauri webview, those context
keys can remain `false` even when the textarea is focused, so default bindings
for arrows and Tab never run.

DCC restores navigation with `installEmbeddedShellNavigationCommands()`:

1. Register a key chord with `editor.addCommand(keybinding, handler)` (no
   `when` clause, so the binding is unconditional).
2. Inside the handler, call `editor.trigger("keyboard", commandId, null)` where
   `commandId` is a **built-in Monaco command** such as `cursorUp`, `tab`, or
   `outdent`.

That pattern uses public Monaco APIs and invokes the same command pipeline Monaco
uses internally. It is a targeted workaround for the embedded shell; it is **not**
equivalent to listening on `window`/`document` and manually moving the cursor.

**Do not replace this with DOM-level `keydown` capture.** That approach was
explored and rejected: it fights Monaco's focus model and is harder to maintain
than `addCommand` + `trigger`.

## File layout

| File | Responsibility |
| --- | --- |
| `apps/desktop/src/lib/monaco-runtime.ts` | Monaco worker setup, themes, `createFileEditor`, `createDiffEditor`, Tauri shell helpers |
| `apps/desktop/src/features/editor/file-tabs-surface.tsx` | Shared Monaco instance across Code dock tabs; buffer and cursor state per path |
| `apps/desktop/src/features/editor/WorkspaceFileSurface.tsx` | Per-tab surface; exports `WorkspaceFileEditor` |
| `apps/desktop/src/features/editor/WorkspaceEditorSurface.tsx` | Git diff surface (side-by-side / inline diff) |

All Monaco-specific shell work should stay centralized in `monaco-runtime.ts`
so the React layer only deals with tabs, buffers, and focus lifecycle.

## Code dock: shared editor and tabs

The Code dock uses **one Monaco instance** for all open tabs. When the active tab
changes, the controller calls `switchFile(path, content, line?, column?)` instead
of destroying and recreating the editor on every tab switch.

This mirrors common IDE patterns (single editor widget, swap document) and avoids
focus and keyboard regressions from repeated mount/unmount cycles.

React-side state in `file-tabs-surface.tsx`:

- **`buffersRef`** — unsaved text per file path
- **`cursorByPathRef`** — last cursor position per file path (restored on tab switch)
- **`editorSwitchLockRef`** — ignores spurious `onChange` events while switching tabs
- **`activateTab()`** — saves the leaving tab's buffer and cursor before changing
  `activePath`

These are application concerns, not Monaco workarounds.

### React StrictMode

`apps/desktop/src/main.tsx` does not wrap the app in `React.StrictMode`. Strict
double-mounting in development was causing Monaco to boot twice and lose keyboard
state. If StrictMode is re-enabled, the editor lifecycle must be idempotent
(single controller per host, safe cleanup on remount).

## Git diff surface

Git diffs use `createDiffEditor()` with the same `editContext: false` setting.
Navigation commands are installed on **both** the original and modified inner
editors.

The inspector git flow shows **diff only** in the workspace panel. Full-file
editing belongs in the **Code** dock. The old "Whole file" toggle was removed to
avoid duplicating that experience.

## Focus helpers

`focusCodeEditor()` calls `editor.focus()` and, when present, focuses Monaco's
`.inputarea` textarea. `releaseCodeEditorFocus()` blurs that textarea when the
editor is disposed. These helpers address focus hand-off between the Tauri shell
and Monaco's DOM; they do not alter editing semantics.

`installCodeEditorFocusGuards()` focuses the editor on mouse down so clicks inside
the editor surface reliably activate the text input.

## Contributing guidelines

When changing editor behavior:

1. **Prefer Monaco options and APIs** (`IEditorOptions`, `addCommand`,
   `trigger`, model APIs) over DOM event interception.
2. **Keep shell-specific logic in `monaco-runtime.ts`** with a short comment
   pointing to this document when adding Tauri-only behavior.
3. **Test in the Tauri app**, not only in a plain browser dev server — context
   key and EditContext behavior differ in the webview.
4. **Verify** arrow keys, Tab, Shift+Tab, typing, and tab switching in the
   Code dock, plus arrow navigation in the git diff view.

If upstream Monaco or Tauri fixes the context-key / EditContext issues, revisit
`editContext` and `installEmbeddedShellNavigationCommands()` and test whether the
extra commands can be removed without regressions.

## Related Monaco documentation

- [Monaco Editor API](https://microsoft.github.io/monaco-editor/docs.html)
- [Standalone editor construction options](https://microsoft.github.io/monaco-editor/typedoc/interfaces/editor.IStandaloneEditorConstructionOptions.html)
- [Adding commands and keybindings](https://github.com/microsoft/monaco-editor/wiki/Keyboard-Shortcuts)
