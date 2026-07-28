# Grok MCP bridge boundary

The DCC Grok MCP path targets Grok Build's ACP server. It does not edit Grok
user configuration, imported Cursor or Claude configuration, plugin
directories, OAuth state, or MCP credential stores.

Status as of July 28, 2026: an exact-version, backend-only ACP projection
builder is implemented and tested offline for stdio and Streamable HTTP. The
production Grok adapter remains `Unsupported` and returns no DCC MCP projection
version. The installed runtime has not yet proven the structured ownership and
permission round trip required to enforce DCC policy safely.

## Why ACP

Grok Build exposes `grok agent stdio` as its ACP transport. ACP `session/new`
accepts session-provided `mcpServers`, so DCC can prepare a private,
non-persistent projection instead of writing provider-owned configuration.

References:

- [Grok Build source](https://github.com/xai-org/grok-build)
- [Grok CLI reference](https://docs.x.ai/build/cli/reference)
- [ACP session setup](https://agentclientprotocol.com/protocol/v1/session-setup)
- [ACP tool calls and permission requests](https://agentclientprotocol.com/protocol/v1/tool-calls)

The documented `--plugin-dir` path is not used for this bridge. Grok treats
plugins supplied that way as trusted, which does not preserve DCC's explicit
per-tool approval boundary.

## Audited runtime contract

The current builder is pinned to:

```text
Grok Build 0.2.101
ACP protocol 1
launch path grok --no-auto-update agent stdio
runtime evidence grok-build@0.2.101+acp-v1
```

The builder requires the ACP `initialize` response to report:

- `protocolVersion: 1`;
- `_meta.grokShell: true`;
- `_meta.agentVersion: "0.2.101"`; and
- `agentCapabilities.mcpCapabilities.http: true` when any Streamable HTTP
  definition is present.

No upgrade or downgrade inherits compatibility. Runtime identity comes from
the authenticated ACP handshake, rather than parsing presentation text from
`grok --version`.

The installed CLI was observed locally, without a model prompt, returning the
contract above plus HTTP and SSE MCP capabilities. This observation proves
only initialization compatibility.

## Private projection

For an eligible session, the inert builder:

1. validates the absolute workspace and stdio executable paths;
2. validates bounded server, argument, environment, header, URL, and explicit
   tool-policy inputs;
3. rejects per-server stdio working directories because ACP represents only
   the session working directory;
4. replaces DCC display names with random per-session wire names;
5. serializes credentials only inside a redacted, zeroizing one-shot
   `session/new` payload; and
6. retains definition ownership and explicit `Allow`/`Deny` policy maps only
   in backend memory.

`Ask` is rejected by the builder while the adapter cannot complete the reverse
permission request. Secrets never appear in argv or debug output.

## Provider-owned MCP coexistence

The installed Grok runtime discovers MCP servers from provider-owned and
external compatibility sources, including enabled Cursor and Claude MCP
configuration. Those servers may coexist with `session/new` servers.

Therefore:

- appearing in Grok's catalog is not proof that DCC owns a server;
- a display title or a guessed `<server>__<tool>` split is not authorization
  evidence;
- DCC ownership must match an exact random wire name through structured ACP
  fields; and
- DCC must never disable, remove, or overwrite provider-owned entries when a
  DCC integration is disabled or removed.

## Approval blocker

The public Grok Build main branch inspected on July 28, 2026 contains promising
structured contracts:

- tool-call `_meta["x.ai/tool"]` identity;
- permission tool-call updates carrying structured metadata and `rawInput`;
- `x.ai/mcp/list`, `x.ai/mcp/servers_updated`, and
  `x.ai/mcp/server_status`; and
- MCP catalog entries with explicit server names, status, and tools.

However, the installed `0.2.101` binary reports a monorepo revision that is not
the public repository revision inspected for those contracts. Main-branch
behavior is not evidence for the exact installed runtime.

The current DCC Grok adapter also treats inbound ACP messages as notifications.
It has no reverse `session/request_permission` response path. Activating MCP
projection now could leave approval-requiring calls unresolved, and accepting
or denying them by title or qualified-name convention would be heuristic.

## Activation gate

The production adapter may expose a DCC MCP projection version only after the
exact Grok runtime can:

1. identify the MCP server and exact tool in structured permission and
   lifecycle data;
2. correlate that identity to a DCC-owned random wire name without relying on
   titles, catalog presence, or delimiter guessing;
3. complete ACP reverse permission requests with one-call `Ask`, `Allow`, and
   `Deny` semantics;
4. distinguish DCC-projected servers from Grok, Cursor, Claude, plugin, and
   managed-gateway servers;
5. publish only provider-observed tool inventory without arguments, results,
   credentials, or raw failure text;
6. stop projected stdio children deterministically when the DCC session ends;
   and
7. pass the shared stdio and HTTP conformance harness.

Until then, users may continue using Grok-native or externally imported MCP
configuration, which DCC neither owns nor modifies.
