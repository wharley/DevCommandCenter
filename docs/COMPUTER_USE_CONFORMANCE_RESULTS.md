# Computer Use conformance results

Authenticated gates were run locally on September 23, 2026. Evidence is
diagnostic only; it does not promote a provider in the DCC catalog.

| Provider | Runtime | Result |
| --- | --- | --- |
| Claude Code | `claude-agent-sdk@0.2.126+claude-code@2.1.280` | Passed both stdio and HTTP image and interruption checks. Evidence verified at `2026-09-23T19:57:15.827410+00:00`. |
| Codex | `codex-cli@0.155.1+app-server-protocol-v2` | Passed both stdio and HTTP image and interruption checks. Evidence verified at `2026-09-23T19:50:38.833513+00:00`. |
| Cursor | Installed `cursor-agent 2026.09.18-9a7762b` | Not certified. The audited DCC runtime remains `cursor-agent@2026.07.23-e383d2b+acp-v1`. The pinned gate refused the installed version. An experimental gate run with the newer version failed to observe MCP tools and a completed, authorized image call, so no evidence was issued. |

Successful evidence records:

```json
{"providerId":"codex","providerVersion":"codex-cli@0.155.1+app-server-protocol-v2","suiteVersion":"dcc-computer-use-provider-conformance-v1","fixtureVersion":"dcc-computer-use-fixture-v1","transports":[{"transport":"stdio","checks":["mcpImageUnderstood","activeTurnInterrupted"]},{"transport":"http","checks":["mcpImageUnderstood","activeTurnInterrupted"]}],"verifiedAt":"2026-09-23T19:50:38.833513+00:00"}
```

```json
{"providerId":"claude_code","providerVersion":"claude-agent-sdk@0.2.126+claude-code@2.1.280","suiteVersion":"dcc-computer-use-provider-conformance-v1","fixtureVersion":"dcc-computer-use-fixture-v1","transports":[{"transport":"stdio","checks":["mcpImageUnderstood","activeTurnInterrupted"]},{"transport":"http","checks":["mcpImageUnderstood","activeTurnInterrupted"]}],"verifiedAt":"2026-09-23T19:57:15.827410+00:00"}
```

The Codex interruption check exposed that the adapter killed the whole process
without confirming the active turn stopped. It now sends App Server
`turn/interrupt` with the active thread and turn IDs. The authenticated gate
then passed interruption on both transports.

The Computer Use fixture now waits for Codex to publish a connected MCP
snapshot containing both test tools before sending the image prompt. Without
that wait, the provider could answer that the tool was unavailable before
MCP setup completed. The OCR fixture now uses reproducibly generated,
high-contrast eight-digit codes instead of words that the model could confuse.
Claude does not publish the same readiness snapshot before its first turn, so
the adapter exercises its tool path directly.

The current Cursor CLI's official ACP guidance documents MCP configured in a
project or user `.cursor/mcp.json`. DCC's inline session configuration was not
observed as connected by the newer CLI during the experimental gates. The
exact-version gate remains in place until the DCC adapter is updated and the
shared MCP and Computer Use contracts pass on a Cursor runtime.

A separate September 23 probe used a credential-free fixture in a private
temporary project's `.cursor/mcp.json` and an ACP session with an empty inline
`mcpServers` list. Cursor emitted and completed `dcc-fixture: fixture.echo`.
It did not emit an ACP permission request for that call, so the result only
confirms the file-based connection path; it does not pass DCC's authorization
contract or the Computer Use gate.
