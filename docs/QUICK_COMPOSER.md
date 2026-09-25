# Quick composer

On macOS, **Command + Shift + C** opens a floating composer over the current
application while DCC is running. Change or disable the shortcut in
**Settings → Shortcuts → Quick composer**, or use **Open** there. Conflicting
shortcuts are reported without taking over the Appshots shortcut.

Existing installations retain their saved shortcut. To adopt the new default,
click the shortcut in these settings and press Command + Shift + C.

Select a registered project, choose **Local** or **Worktree**, pick a provider and
model, and send a message. Local uses the project's original directory and current
branch, including uncommitted changes. Worktree uses DCC's existing isolated task
creation flow and the project's configured base branch. The panel remembers the
environment separately for each project; a project without a saved choice starts
with Worktree. Concurrent Local tasks share the same files.

The existing composer supplies rich text, file/image attachments, Appshots,
planning, effort, and approval settings. Type `@` to search Git-tracked files in
the selected project before creating a task. Mentions keep project-relative paths,
so `@src/App.tsx` is resolved in the eventual Local directory or Worktree. The
suggestions describe the current project checkout; files only present on that
branch may not exist in a Worktree created from another base branch. File previews
use the selected project. The suggestion list stays within the floating panel.

Attachments use absolute paths because
the execution worktree does not exist yet. Appshots picker captures go only to
this panel's draft; the independent Appshots global shortcut continues to target
the main conversation. Drafts are separate for each project and from the main
conversation. Escape, the close button, and the global shortcut hide the panel
without discarding its draft. Sending starts a new session and hides the panel
after the first turn is accepted. Reopen the panel and use **Open in DCC** to
navigate to the task. On reopen, the panel checks the current task list and removes
the previous-task notice when that task is completed, archived, or removed. Every
send creates a new task; it never resumes the last task implicitly. Uncertain
launches retain their separate recovery gate.

Model selection uses a single dialog bounded by the panel, with a searchable
list, shared favorites, effort, and response style. Lists scroll inside the dialog
instead of opening side submenus. Returning to the request restores editor focus.

DCC must remain running. On macOS, closing the main window hides it while the
[menu bar companion](MENU_BAR.md) remains available. **Quit DCC** / Command + Q
ends the companion surfaces too. If the menu bar fails to initialize, closing
the main window keeps the previous shutdown behavior.

## Delivery and recovery

Before creation and each subsequent step, DCC saves a launch journal under its
application data directory in `quick-composer/state.json`. It preserves the prompt,
execution choices, progress, and any known workspace/session identifiers. Revision
checks prevent two invocations from claiming the same request. The panel uses
the normal workspace/session commands and the existing MCP authentication preflight;
it does not create a separate provider runtime or bypass approval policies.

An interrupted creation or uncertain send is **never replayed automatically**.
The panel preserves the request and offers **Open in DCC** and **Copy request**.
Check the partial task before explicitly allowing a new request. A failure during
session startup may have persisted a session before returning an error; inspect
the task's conversations. If a creation response was lost before its identifier
was saved, find the task in the main project list. Earlier launch journals are
retained as files named by request ID when starting the next request.

## Native implementation

The hidden Tauri WebView is created during startup. A macOS `NSPanel`, created with
public AppKit APIs, hosts its content view. The panel joins Spaces and is configured
as a full-screen auxiliary window. It can receive keyboard focus without becoming
the app's main window. No private activation selector or runtime class replacement
is used. The native host is restored before destruction. The quick entry route
mounts only its own React surface and uses an independent, non-persisted query
cache; it does not mount a second workbench.

The shared global-shortcut handler dispatches by registered shortcut. New panel
commands accept only DCC's main/quick-composer WebViews as appropriate. Panel
navigation is restricted to the local frontend entry. Appshots checks the panel's
draft namespace, and drag/drop subscriptions are scoped to the receiving WebView.

## Validation

- Frontend delivery tests cover Local/Worktree, repeated and concurrent delivery,
  uncertain sends, startup errors, and checkpoint failures.
- Rust tests cover journal persistence, revision/phase checks, immutable task
  identity, and input validation; existing Appshots tests cover draft delivery.
- The isolated browser fixture is
  `/apps/desktop/tests/quick-composer.html` on the root OJ dev server. `?failure=1`
  simulates provider startup failure; `?empty=1` shows the empty project state.
  It never starts a real provider or reads DCC's database.
- Native smoke testing uses a separate app identifier/data directory. Release
  validation should also check a physical global shortcut from another app,
  returning focus, full-screen Spaces, multiple monitors, and a real provider
  first turn in both environments. Browser mocks cannot validate those behaviors.

The product reference was MonoCode's public quick composer. This implementation
uses DCC's own composer, task services, and a separate AppKit panel host.
