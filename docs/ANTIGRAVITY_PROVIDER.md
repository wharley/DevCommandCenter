# Antigravity provider

Personal Google sign-in is no longer routed through Gemini CLI. DCC exposes
Antigravity as a separate provider and keeps Gemini CLI as a legacy provider
for API-key, Vertex AI, and eligible enterprise configurations.

## Setup

1. Open **Settings > Models** and find **Antigravity**.
2. Select **Install official runtime**. DCC downloads the platform artifact
   published in the [ACP Registry][registry], verifies its exact byte length
   and SHA-256 digest, extracts only `agy_acp_server` and its matching
   `localharness_external`, then verifies the ACP identity and release version
   before activation.
3. Select **Sign in with Google** and complete the browser flow. DCC verifies
   account access by creating a temporary ACP session and reads the models
   offered to that account. After the first successful connection, DCC restores
   the verified account status and model list across application and development
   server restarts; **Sign in with Google** becomes **Refresh models**.
4. Select Antigravity and an available model for a new DCC session.

The macOS download is available only for Apple Silicon. Linux supports x64 and
ARM64; Windows supports x64 and ARM64. A manually installed official runtime
can be selected with **Executable path**. The helper must be executable and in
the same directory. An invalid explicit path fails closed and never falls back
to a different installation.

## Security and runtime behavior

- Antigravity uses the official `agy_acp_server` protocol process, not the
  transcript/polling integration used by Synara.
- Google credentials are stored by the agent in an isolated, permission-
  restricted DCC profile. Ambient Gemini and Google credential variables are
  removed before launch.
- DCC persists only the profile path, verification time, and non-secret model
  metadata. It never copies OAuth tokens into its own account-state cache, and
  stops presenting the cached account as connected when the agent token is
  absent.
- The runtime does not receive terminal access through ACP.
- File reads and writes are restricted to the active workspace root. The
  official agent applies the permission mode before asking DCC to perform a
  write, avoiding a duplicate approval for the same edit.
- DCC's existing approval policies map directly to Antigravity's negotiated
  ACP modes: **Request approval** uses `default`, **Approve for me** uses
  `auto_edit`, and **Full access** uses `yolo`.
- In supervised and automatic modes, tool permissions use the exact option IDs
  supplied by the ACP agent. Full access resolves any exceptional native
  request with an offered allow option instead of opening another prompt.
- Models are discovered from the authenticated ACP session. DCC never silently
  substitutes a model that is absent from the current account.
- Gemini CLI sessions are not silently converted to Antigravity sessions.
- Additional workspace directories, MCP projection, native plan mode, and
  conversation resume remain disabled until the official agent advertises a
  contract that DCC can verify for each feature.

The current managed release is pinned in
`crates/dcc-tauri/src/antigravity_installation.rs`. Update the URL, archive
length, extracted file lengths, SHA-256, and expected ACP version together
when adopting a newer Registry release.

[migration]: https://antigravity.google/docs/cli/gcli-migration/
[registry]: https://github.com/agentclientprotocol/registry/blob/main/antigravity-acp/agent.json

See Google's [Gemini CLI migration guide][migration] for the upstream account
migration context.
