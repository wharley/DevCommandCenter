# Synara Computer Use: decision for DCC

Reviewed Synara at [`eaa61eded31b6755d4f30ba8eabc5d905cf817cb`](https://github.com/Emanuele-web04/synara/tree/eaa61eded31b6755d4f30ba8eabc5d905cf817cb) on 2026-09-23. This is an architecture comparison, not a claim that Synara's release or every provider is certified.

## Decision

**Yes, Synara meets the product and architecture direction** for a provider-neutral Computer Use surface. The application owns consent, native control, target identity, cancellation, and a local preview. Provider adapters receive tools conditionally. Preview frames are a separate local channel and are not automatically sent to the model. Its documented macOS release is beta; the repository explicitly says live task completion across all nine adapters remains unverified.

**No, Synara does not supply a proven shortcut for the DCC Cursor ACP gate.** Its Cursor adapter also builds session-level ACP `mcpServers` entries through a shared gateway. The DCC conformance run with the installed Cursor CLI did not observe those tools. A preview stream cannot repair a missing provider tool connection. Keep Cursor unavailable for DCC Computer Use until its authenticated image and interruption gate passes on an exact supported runtime. Stop experimenting with private `.cursor/mcp.json` roots as an implicit workaround.

Sources: [Synara implementation notes](https://github.com/Emanuele-web04/synara/blob/eaa61eded31b6755d4f30ba8eabc5d905cf817cb/docs/computer-use-cua/README.md), [preview architecture](https://github.com/Emanuele-web04/synara/blob/eaa61eded31b6755d4f30ba8eabc5d905cf817cb/docs/computer-use-cua/native-preview.md), [Cursor adapter](https://github.com/Emanuele-web04/synara/blob/eaa61eded31b6755d4f30ba8eabc5d905cf817cb/apps/server/src/provider/Layers/CursorAdapter.ts), [MCP injection](https://github.com/Emanuele-web04/synara/blob/eaa61eded31b6755d4f30ba8eabc5d905cf817cb/apps/server/src/agentGateway/mcpInjection.ts), [DCC runtime evidence](COMPUTER_USE_CONFORMANCE_RESULTS.md).

## Scope of this DCC increment

- DCC already has macOS 14+ exact-window capture and an allowlist tied to the provider session lease. The UI already shows the most recent explicit MCP capture and activity events.
- The preview now refreshes the same authorized PID/window while its conversation turn is active and visible. It stops pulling when hidden, when the turn ends, when the grant expires, or when the target disappears. A window move or title change can be followed because the native capture validates the current geometry.
- Refresh is a renderer-only read. It never creates a new action generation, never alters the provider's captured action target, and never sends its frame to the model. The explicit `dcc_computer_capture` remains the only way to ground agent coordinates.
- The transport is a bounded one-second still refresh in the local Tauri UI. It is not Synara's 15 fps native helper or a general video stream. DCC's existing four-megabyte/window capture limit still applies.

## Remaining completion gates

1. Qualify the packaged DCC macOS build with Screen Recording and Accessibility grants: approval, explicit capture, moving-window preview, Stop, revocation, and a provider action. Source review alone cannot certify native behavior.
2. Keep the provider matrix exact-version based. Codex and Claude passed the current stdio/HTTP image and interruption fixture; Cursor remains unavailable until its DCC MCP and Computer Use contracts pass.
3. Only consider a faster native frame tap after measuring this still channel's CPU, memory, and capture latency in the packaged app. A browser or video stream should be a separate target-specific source with the same lease and consent checks.
4. Synara's richer semantic targeting, browser automation, foreground control, and cross-platform native backends are separate product work. Their existence does not qualify DCC's current macOS capture or other operating systems.
