# Desktop Computer Use on macOS

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

**Pointer input** is real HID input posted to `kCGHIDEventTap`, as when a person
clicks. It is the only path that reaches web content and custom-drawn controls
that publish no accessibility action. Before posting, DCC brings the exact
window to the front with keyboard focus: AX raise, AX main, AX frontmost, and
app activation. These public controls work while DCC itself is not the active
app, and they select the right window of a multi-window app. DCC then proves
focus and that the point is not covered. If another app's window covers the
point, the action fails with that app's name. The target app's own overlays may
cover it, because they are part of the observation.

**Keyboard input** is posted to the target process, never to whichever app is
frontmost later, so a focus change in the middle of the text cannot leak
keystrokes elsewhere.

Synthetic events posted to a process without focus, and private SkyLight
APIs, are deliberately not used. Chromium ignores the former. The latter are
unsupported and can break with any macOS update.

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
