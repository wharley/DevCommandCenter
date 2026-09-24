# macOS menu bar

While DCC is running, its monochrome symbol appears in the macOS menu bar.
Click it to open an activity popover. Click outside, press Escape, or click the
symbol again to dismiss it. AppKit anchors the popover to the status item on the
current screen and handles its appearance and dismissal.

The panel shows local tasks needing approval or a reply first, then running tasks,
then up to five recent completed/interrupted tasks. Each workspace is counted
once, even when it has multiple conversations. Clicking a task opens the relevant
conversation in the main window. An active but idle conversation is not counted
as running. An interrupted turn is not necessarily an error; user cancellation
also interrupts turns. Approval/reply status comes from unresolved permission and
user-input requests in the current turn.

**New task** opens the existing quick composer, preserving its draft if already
open. **Open DCC** restores the main window. The main window's close button hides
it while the menu bar is available. Clicking the Dock icon restores it.
**Quit DCC** and Command + Q retain normal application shutdown behavior.
No login item or automatic startup is installed.

## Resource scope and refresh

CPU and memory sum the DCC app, its daemon, and their descendant processes, with
process IDs deduplicated. This includes providers/build commands that remain in
those process trees. External services, detached processes, and WebKit processes
managed by macOS may be outside this scope. It is not total Mac resource usage.
Memory is resident memory in MiB/GiB. CPU uses the per-core convention: 100% is one
logical core, so the aggregate may exceed 100%. A fresh CPU sampler displays a
dash until a second sample is available.

Activity and resource sampling refresh every three seconds **only while the
popover is open**. Database work and process sampling run off the main thread.
Failures show an explicit unavailable/stale notice instead of implying that no
tasks are running. SQL reads a consistent transaction and selects only task
metadata and lifecycle/request controls; no conversation text is returned.

## Implementation and verification

A separate, hidden Tauri WebView renders the compact React surface. A native
`NSStatusItem` and transient `NSPopover` host that view through public AppKit APIs.
The symbol derives from the app's existing vector mark and uses template rendering.
No additional workbench or provider runtime is mounted. Companion commands accept
only the menu-bar WebView, and navigation is restricted to the local entry point.
On shutdown the native host is restored before the WebView is destroyed.

- Rust tests: request matching, terminal cleanup, late events from older turns,
  grouping/priority, archived tasks, process ancestry and deduplication.
- React tests: visibility-bound polling, cleanup, errors/recovery, CPU warmup,
  navigation, composer, Escape and explicit quit.
- Browser fixture: `/apps/desktop/tests/menu-bar.html` on the OJ dev server.
  `?theme=light`, `?empty&lang=en`, and `?error` exercise isolated states.
- Browser smoke: `node apps/desktop/tests/menu-bar-smoke.mjs`. Optional variables:
  `DCC_PLAYWRIGHT_MODULE`, `DCC_CHROMIUM_EXECUTABLE`, `DCC_MENU_BAR_URL`,
  `DCC_MENU_BAR_SCREENSHOTS`. Uses synthetic tasks and no real providers.
- Native release smoke still needs physical clicks on the status item, keyboard
  focus/Escape, full-screen Spaces, multiple monitors, close/reopen and explicit
  quit in a separate app identifier/data directory. Browser fixtures cannot
  validate those OS behaviors.
