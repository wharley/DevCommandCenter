# Browser Workbench

The Browser Workbench keeps local web development, agent collaboration, and debugging
evidence inside the current DCC workspace. It uses a native Tauri child WebView and a
provider-neutral MCP bridge with explicit, short-lived permissions.

This document describes the capabilities implemented in DCC. It is not a product
roadmap.

## What is implemented

### Native Browser surface

- One native child WebView is owned and controlled by the Tauri backend.
- The Browser shares the workbench with chat and the integrated terminal.
- Its lifecycle is scoped to the current workspace and session.
- Responsive split layout, resize, temporary UI occlusion, close/reopen, and stale
  callbacks are handled without giving remote pages access to DCC IPC.
- Pop-ups and downloads are denied. Navigation accepts HTTPS and local HTTP addresses.

### Address input and local servers

The human-facing address bar behaves like a small omnibox:

- explicit HTTP(S) URLs are preserved;
- `localhost` and loopback addresses gain HTTP when the scheme is omitted;
- domain names gain HTTPS;
- words and phrases become a web search;
- filesystem-looking input is not sent to the search provider.

Agent navigation does not use these conveniences. It must provide an explicit URL that
passes backend validation.

The integrated terminal detects safe HTTP(S) loopback URLs printed by development
servers and offers an explicit **Open** action. Detection handles ANSI terminal output,
URLs split across PTY chunks, IPv4/IPv6 loopback, and bind-all addresses. It does not
infer remote or LAN destinations.

### Scoped agent control

The person can grant Browser control for at most 60 seconds. DCC keeps the grant bound
to workspace, session, Browser lifecycle, and the projected provider lease.

The provider-neutral Browser tools support:

- reading bounded page context;
- navigating and reloading;
- scrolling;
- clicking safe interactive controls;
- filling safe text inputs;
- starting and reading an explicit evidence capture.

Click and fill use opaque references from a fresh bounded semantic map. DCC does not
give providers arbitrary CSS selectors, raw DOM access, or a general JavaScript
execution capability.

### Page, console, and network evidence

An explicit evidence capture includes:

- a bounded snapshot of the safe selection or visible page text at capture start;
- console warnings and errors emitted after capture starts;
- global errors and unhandled promise rejections;
- resource failures;
- allowlisted Resource Timing metadata when the platform makes it available.

Visible-text extraction traverses a limited number of open Shadow DOM roots so that
development overlays, including Next.js error UI, can be represented. Every page value
is treated as remote, untrusted data.

Captures are short-lived, bounded, redacted, and one-shot. A full navigation, reload,
document replacement, expired grant, lifecycle change, or scope change invalidates the
capture. A same-document SPA route change can remain valid when it stays on the same
origin.

### Evidence workflow

Browser evidence enters the composer evidence tray instead of being pasted into the
message editor. The available stages are:

```text
Observe -> Reproduce -> Investigate -> Fix -> Verify
```

After a turn that contains evidence completes, DCC can offer the same item for explicit
reattachment in the next stage. This supports a separate verification turn after a fix.

Evidence bodies remain ephemeral in the renderer. Durable timeline records contain only
bounded metadata such as stage, source, trust, character count, and truncation state.

### Runtime audit and URL persistence

- Browser tool activity has a bounded, content-free audit view scoped to the active
  workspace, session, and lifecycle.
- Audit records contain operation metadata, not page content, URLs, input text, tokens,
  or credentials.
- The last ready Browser URL can be restored for a workspace/session after validation
  and sanitization.
- Grants, semantic maps, evidence handles, page wrappers, and audit records are not
  restored after restart.

## Security and privacy boundaries

- Remote page content is data, never trusted instructions.
- Browser access always remains within an active DCC workspace/session scope.
- Evidence URLs exclude embedded credentials, query strings, and fragments.
- Console arguments and objects are not collected.
- Request/response bodies, headers, cookies, and tokens are not collected.
- Password fields, control values, raw DOM, and arbitrary selectors are not exposed.
- Evidence is not recorded continuously in the background.
- Browser capabilities are not made available to a provider unless its DCC MCP
  projection is explicitly supported and active.

## Current limitations

- Resource Timing is best-effort and is not equivalent to native network interception.
- Screenshots are not part of the current evidence capture.
- The main DCC Web Inspector does not inspect the separate Browser child WebView.
- Closed Shadow DOM and cross-origin iframe content are not inspected.
- DCC is not intended to provide full Chrome DevTools compatibility, browser extensions,
  downloads, or unrestricted web automation.
- Platform-specific behavior still requires validation in packaged macOS, Windows, and
  Linux builds.

## Manual smoke test

1. Start a local development server in the integrated terminal.
2. Use the detected server's **Open** action and confirm that the page appears in the
   Browser.
3. Navigate to a page with a visible development error.
4. Grant agent control for 60 seconds.
5. Start evidence capture, reproduce the issue when necessary, and select **Collect**.
6. Attach the evidence under the appropriate stage and ask the agent to diagnose or fix
   the problem.
7. After the fix turn completes, choose **Reattach to verify**.
8. Reload or navigate to the corrected page, start a fresh capture, and send the verify
   turn.
9. Confirm that closing the Browser does not close DCC, including when the local server
   has stopped or the page cannot be loaded.

The release candidate has completed this principal flow on macOS with a local Next.js
application and an already-visible development error overlay. This smoke does not
replace negative-path, budget, packaged-build, or cross-platform validation.

## Implementation references

- Browser backend and native WebView lifecycle:
  [`src-tauri/src/browser_commands.rs`](../src-tauri/src/browser_commands.rs)
- Browser React surface:
  [`apps/desktop/src/features/browser/workspace-browser-surface.tsx`](../apps/desktop/src/features/browser/workspace-browser-surface.tsx)
- Browser evidence formatting:
  [`apps/desktop/src/features/browser/browser-agent-context.ts`](../apps/desktop/src/features/browser/browser-agent-context.ts)
- Local development server detection:
  [`apps/desktop/src/features/terminal/dev-server-detection.ts`](../apps/desktop/src/features/terminal/dev-server-detection.ts)
