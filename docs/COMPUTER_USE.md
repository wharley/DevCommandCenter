# Desktop Computer Use on macOS and Linux

How an agent observes and operates an external app window that the user
authorized. The trust model (lease, approval, allowlist) is unchanged from
[MCP_TRUST_MODEL.md](MCP_TRUST_MODEL.md); this document covers observation and
input.

## Observe, act, observe

`dcc_computer_capture` returns three things for one allowed window:

- a PNG of the window at nominal resolution. When the app has its own menus,
  popovers, or autocomplete lists open over the window, they are composed into
  the same image, because they are separate windows that a single-window
  capture would miss;
- an element list, one accessibility element per line:
  `e12 button "Salvar" [press] @420,310 80x28`. Coordinates are window-local
  pixels of the screenshot. The app's open context menus are listed after the
  window under `open menu:`;
- a one-shot target generation.

Every action consumes the generation and returns the window as it looks
afterwards, with a new generation. That observation waits 350 ms for the UI to
react and up to 3 s more while a web area reports itself busy. The agent sees
the effect of each action without a separate capture round trip.

Refs (`e1`…`e800`) are valid only with the generation that listed them. The
elements themselves stay in the DCC process (`DCCAXTree` in
`computer_use_macos.m`); the agent can only name what DCC observed in the
authorized window. An element map is released when its capture is consumed,
replaced, or dropped with the grant.

## Input methods

| Tool | By ref | By x/y |
| --- | --- | --- |
| `dcc_computer_click` | AXPress (left) or AXShowMenu (right) when the element offers it; otherwise a pointer click at the center of its visible part | pointer click |
| `dcc_computer_scroll` | pointer wheel at the element's visible center | pointer wheel |
| `dcc_computer_drag` | — | pointer press, 16 intermediate drags, release |
| `dcc_computer_set_value` | AXFocused then AXValue on an editable element | — |
| `dcc_computer_type` / `dcc_computer_key` | — (keyboard focus) | — |

**Accessibility actions** are exact, do not move the user's cursor, and work
without bringing the app forward.

**Background input** comes first for clicks by point, scroll, drag and keys.
The events go to the target process and window through SkyLight
(`SLEventPostToPid`), stamped with the window id and the window-local point.
The window is not raised and the person's cursor does not move, so a window
covered by another app is still reachable. Before a click or drag, DCC posts
the window server's focus record to the target only, so the person's front app
keeps its keyboard. Clicks start with an off-window click at (-1, -1), which
Chromium and Electron need before they accept a click as a user gesture. Keys
wait until the target is its app's key window and stop if it never becomes
one. The technique is ported from trycua/cua (MIT), `native/computer_use_background.m`.

The SkyLight symbols are private and resolved at runtime. When any is missing,
or the focus record is refused, the action takes the foreground path instead.
Each result says which path it took (`accessibility`, `background`,
`foreground`). Some apps ignore background input (canvas apps such as Blender,
Unity and games). When an action had no visible effect, the agent repeats it
once with `foreground: true`.

**Foreground input** is real HID input posted to `kCGHIDEventTap`, as when a
person clicks. Before posting, DCC brings the exact window to the front with
keyboard focus: AX raise, AX main, AX frontmost, and app activation. These
public controls work while DCC itself is not the active app, and they select
the right window of a multi-window app. DCC then proves focus and that the
point is not covered. If another app's window covers the point, the action
fails with that app's name. The target app's own overlays may cover it,
because they are part of the observation.

**Keyboard input** is posted to the target process, never to whichever app is
frontmost later, so a focus change in the middle of the text cannot leak
keystrokes elsewhere.

## Web content (Chromium, Electron)

Chromium and Electron build the accessibility tree of web content only for
assistive technology. On the first capture of such a process, DCC sets
`AXManualAccessibility` on the app. This is the attribute those runtimes
document for this purpose, and unlike `AXEnhancedUserInterface` it does not
change window behavior. DCC then polls the tree for up to 1.2 s until it stops
growing. DCC switches the attribute off again only for processes it switched on
itself, once no live grant allows that bundle. A background reconcile catches
expiry.

## Window identity

An action proceeds only if the window keeps the captured process, window-server
id, and geometry. The title is excluded: a browser retitles its window on every
navigation, and that is the content changing, not the window being replaced.

## Grant lifetime

A grant ends after 10 minutes without a capture or action by the agent, and 1
hour after approval at most. The user can revoke it at any time.

One approval covers the task. When its grant lapses, or the task gets a new
provider runtime, the next request re-arms the apps the person approved for
that task, keeping those still running, without a card. Full access does not
skip the card: pointer input moves the person's cursor and brings windows
forward. Turning control off ends the task's approval, as does restarting DCC.
Requests wait five minutes; Escape does not deny, and when DCC is not the
focused window the Dock icon bounces and a system notification names the
request.

## Failure messages

Native effects return a status code (`DCC_COMPUTER_ERR_*` in
`computer_use_macos.h`). The agent receives a sentence that says what to do
next, such as "Slack covers that point of the target window…" or "that element
no longer exists; use a ref from the latest capture". It no longer receives a
bare "click failed".

## Manual qualification

Unit tests cover the Rust contract with a fake native layer. The native layer
needs a signed DCC build with Screen Recording and Accessibility granted. In
development, macOS attributes the grant to the launching terminal or editor.
Qualify each release on:

1. **Arc or Chrome on a heavy web app** (for example, Google Cloud Console): the
   capture lists links and buttons of the page. A ref click on a navigation item
   navigates through AXPress, and the result shows the new page. A ref click on a
   div-only control falls back to the pointer. Scroll by `deltaY` 600 moves the
   page. `set_value` fills a search field, and the page reacts.
2. **Native AppKit app** (Notes or Finder): ref clicks use AXPress. Right-click
   opens a context menu that appears in the next screenshot and under
   `open menu:`. Then click a menu item by ref.
3. **Electron app** (Slack or VS Code): on the first capture, the element list
   grows past the window chrome.
4. **Occlusion:** place another app's window over part of the target, and click
   there by x/y. The error names that app.
5. **Background start:** with the user working in another app, a pointer click
   brings the target window to the front and lands.
6. **Grant end:** after revoking, Accessibility Inspector shows the app's
   `AXManualAccessibility` back to false, when DCC was the one that switched it
   on.

## Linux

The same contract runs on Linux behind the same native trait
(`crates/dcc-server/src/computer_use_linux/`), with the same three steps per
action. Each result still says which one was used.

| | X11 and XWayland windows | Native Wayland windows |
| --- | --- | --- |
| Inventory | EWMH (`_NET_CLIENT_LIST_STACKING`, `_NET_WM_PID`, `WM_CLASS`) | AT-SPI frames of apps with no X11 window |
| Capture | `GetImage` on the window | Screenshot portal: the whole screen |
| Elements | AT-SPI 2, boxes in window pixels | AT-SPI 2, without boxes (Wayland does not expose where a window is) |
| Accessibility step | AT-SPI `DoAction` (by ref, or a plain click by x/y on a button, link, check box or tab), `SetTextContents`, `InsertText` at the caret | the same, by ref only |
| Background step | `XSendEvent` to the window: the cursor does not move and the window is not raised. Toolkits on XInput2 (GTK 3, Qt 5+, Chromium) usually ignore it | none |
| Foreground step | `_NET_ACTIVE_WINDOW`, then XTEST, only with `foreground: true` | RemoteDesktop portal, approved once by the user, only on the active window |

- An app's id is its `WM_CLASS` class (X11) or its AT-SPI name (Wayland),
  lowercased.
- `org.a11y.Status.IsEnabled` takes the place of `AXManualAccessibility`. It is
  global, so DCC switches it off only after the last app it switched it on for.
  If a screen reader turned it on, DCC leaves it alone.
- COMMAND acts as Control.
- `dcc_computer_status` carries an `inputNote` that tells the agent all of this
  for the session it runs in.
- The agent's cursor is an override-redirect X11 window with an empty input
  shape. DCC runs on X11, or on XWayland in a Wayland session. On a session
  with no X server the cursor stays off, because Wayland lets no client place a
  window at a screen point or keep it above other apps.

The `live_*` tests in `computer_use_linux/mod.rs` drive a real X11 desktop
(Xvfb, openbox, zenity, AT-SPI) with `--ignored`.
