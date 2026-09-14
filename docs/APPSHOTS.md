# Appshots

Appshots adds screenshots of open macOS application windows to a DCC conversation.
The current implementation supports macOS 14 or later and providers through the
existing composer image-attachment pipeline.

## Use

1. Open a task in DCC.
2. Choose **+ → Capture app window** in the composer.
3. Allow **Screen Recording** for DCC if prompted. Return to DCC; macOS may require
   restarting the app after granting access.
4. Select up to six windows. The picker loads previews six windows at a time.
5. Choose **Attach**, then send your prompt when ready. The exact preview images
   are attached; attaching does not send a message.

To capture the app currently in front, press **Command + Shift + 9** while a
conversation is open in DCC. DCC captures before bringing its window forward and
adds the screenshot to that conversation's draft. Change or disable the shortcut
in **Settings → Shortcuts**, or in the Appshots picker. If another application
already registered the shortcut, choose a different combination.

Providers declaring no vision support receive the existing file-reference
fallback, with a notice in the composer. This does not give those models vision.

## Implementation

- `src-tauri/native/computer_use_macos.m`: existing ScreenCaptureKit capture and
  window inventory. The new frontmost-process lookup selects the global shortcut
  target. DCC's own windows are excluded.
- `src-tauri/src/computer_use_commands.rs`: narrow read-only helpers reuse native
  capture without creating a Computer Use grant. Screenshots need Screen Recording;
  the Appshots flow does not require Accessibility.
- `src-tauri/src/appshot_commands.rs`: main-webview-only commands, bounded preview
  cache, batch attachment persistence, draft-specific delivery and global shortcut
  configuration through the official `tauri-plugin-global-shortcut` plugin.
- `AppshotsDialog`: selection and previews using the existing dialog components.
- `AppshotsPlugin`: delivery into the existing Lexical image/file nodes and
  structured draft persistence. Captures remain addressed to the original draft
  if it unmounts or the user switches tasks while capture is running.

Previews remain in memory, expire after ten minutes and are bounded to 24 entries,
with at most 4 MiB per image. Refresh the picker if a preview has expired or been
evicted. Only explicitly attached images are saved under the app data directory's
`appshots` folder. Pending deliveries and shortcut preferences are persisted there;
images remain on disk for drafts and conversation history. They are not written to
Git worktrees or automatically uploaded. Removing an attachment from a draft does
not delete its underlying local file.

This version captures images. It does not extract Accessibility text, continuously
record the screen, or enable agent control. Linux, Windows, and the mobile/web
companion do not yet support Appshots capture. Very large or unavailable windows
can fail the existing native capture bounds; refresh or resize the window.

## Validation

The focused frontend tests cover preview pagination, multiple selection, permission
and capture failures, draft persistence, retries, provider fallback and a task switch
during asynchronous delivery. Rust tests cover exact captured bytes, draft isolation,
expired/invalid selections and rollback on failed persistence. Native capture is
compiled using the existing bridge.

For a native end-to-end check, run the desktop app, capture an external application
through both the picker and shortcut, inspect the attached image, and send a prompt
to a provider with vision support. Also check permission denial, shortcut conflicts,
restart/draft restoration, and switching tasks during capture.
