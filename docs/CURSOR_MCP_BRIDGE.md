# Cursor MCP bridge

The Cursor bridge is being delivered through the Agent Client Protocol (ACP).
DCC does not edit Cursor's user or project `mcp.json`, does not register an
extension, and does not infer support merely because Cursor can use MCP from
its own configuration.

Status as of July 28, 2026: the production provider is hybrid. Sessions without
a DCC MCP projection retain the existing Cursor `stream-json` path. Sessions
with a projection use `cursor-agent acp` only when the installed CLI exactly
matches the audited version. Other versions receive no DCC definitions.

The catalog reports `NativeConfig`, not `VerifiedBridge`. The shared
authenticated conformance gate is implemented but remains ignored and has not
been run.

## Why ACP

ACP v1 defines session-owned MCP injection in `session/new.mcpServers`. It
requires stdio support and gates HTTP and SSE transports on capabilities from
the `initialize` response. This gives DCC a session-bound path without changing
user-owned Cursor files.

References:

- [ACP session setup and MCP transports](https://agentclientprotocol.com/protocol/v1/session-setup)
- [ACP tool calls and permission requests](https://agentclientprotocol.com/protocol/v1/tool-calls)
- [Cursor CLI MCP behavior](https://docs.cursor.com/en/cli/using)
- [Cursor CLI parameters](https://docs.cursor.com/en/cli/reference/parameters)

Cursor's documented CLI flow automatically reads `mcp.json`. That native path
is intentionally independent from DCC ownership: accepting or listing a native
Cursor server does not prove that `session/new.mcpServers` was honored.

## Current audited contract

The first audited runtime is:

```text
cursor-agent 2026.07.23-e383d2b
ACP protocol 1
```

The projection builder:

- requires that exact Cursor CLI version and ACP protocol version;
- accepts stdio only with an absolute executable and no per-server working
  directory, because ACP v1 cannot represent a stdio server `cwd`;
- accepts Streamable HTTP only when the agent advertises
  `agentCapabilities.mcpCapabilities.http`;
- sends additional workspace roots only when the agent advertises
  `sessionCapabilities.additionalDirectories`;
- validates DCC-owned server names, arguments, environment names, URLs, and
  headers;
- uses randomized wire names and keeps a backend-only mapping to DCC definition
  IDs;
- keeps explicit tool policies outside the vendor payload for later approval
  enforcement; and
- serializes credentials only into a one-shot redacted payload that is zeroized
  after its stdin write.

An upgrade or downgrade does not inherit compatibility. The new exact version
must be audited and must pass conformance before DCC can activate its
projection.

## Production routing and permission boundary

At provider construction, DCC executes only `cursor-agent --version`. The
result enables the internal projection channel only for the exact audited
version. This support probe never treats the generic `agent` command as Cursor;
that command may belong to another provider.

When no eligible DCC definition resolves for the session, the bridge delegates
to the existing Cursor CLI adapter unchanged. When definitions are present, it:

1. starts `cursor-agent acp` with piped stdin/stdout and discarded stderr;
2. initializes ACP v1 and requires the `cursor_login` authentication method;
3. writes the one-shot sensitive `session/new` request and immediately
   zeroizes it;
4. keeps definition ownership and explicit tool policies in backend-only
   maps; and
5. cancels pending permission requests with the ACP `cancelled` outcome.

MCP ownership is accepted only when structured ACP fields contain both the
exact randomized DCC wire server name and a bounded tool name. Human-readable
titles, suffixes, annotations, and guessed naming conventions never establish
ownership.

An owned `session/request_permission` must also correlate with an active owned
tool call. Unknown, malformed, ambiguous, or uncorrelated requests are
cancelled. DCC selects only `allow_once` or `reject_once`; it never converts a
single approval into Cursor's `allow_always` or `reject_always`.

Unknown tools default to `Ask`. Explicit `Allow` and `Deny` policies are
enforced only after the same ownership correlation succeeds.

## Runtime truth

Successful `session/new` produces `AttachingProvider`, not `Connected`.
Acceptance of configuration is not proof that the model sees a tool.

A definition becomes `Connected` only after Cursor emits a structured,
DCC-owned tool call. The runtime inventory then contains only the exact
observed tool. Arguments, raw input, tool output, stderr, and MCP deltas are not
copied into that status or into MCP tool events.

This observed inventory is deliberately incremental. It is not presented as a
complete provider inventory and cannot create conformance evidence.

## Local protocol observation

During development, the audited Cursor CLI:

1. completed ACP `initialize` and `authenticate`;
2. advertised ACP protocol 1 plus HTTP and SSE MCP capabilities;
3. accepted the repository-owned stdio fixture in `session/new`;
4. started the exact `dcc-mcp-fixture stdio` child process; and
5. stopped the child when the ACP process was cancelled.

No model prompt or external MCP service was used. This observation proves the
current injection and child-lifecycle path, but it is deliberately not stored
as conformance evidence: it does not prove tool inventory, tool calls,
permission mediation, disable/remove behavior, or both transports.

## Remaining verification gate

The production adapter now:

- starts the exact Cursor executable without a generic-command fallback;
- sends the private ACP projection;
- correlates structured tool and permission events;
- enforces per-tool policy fail-closed; and
- reports only observed runtime truth.

The remaining release gate is the ignored, authenticated shared conformance
test:

```sh
DCC_RUN_CURSOR_MCP_CONFORMANCE=1 \
  cargo test -p dcc-mcp-fixture --test provider_conformance \
  authenticated_cursor_bridge_passes_the_shared_harness -- --ignored --exact
```

Do not run this gate in fork CI. It requires the exact audited Cursor CLI and
an authenticated account. Until it passes both fixture transports and the
approval lifecycle, Cursor remains `NativeConfig` and receives no
`VerifiedBridge` evidence.

Users may continue using Cursor-native MCP configuration, which DCC neither
owns nor modifies.
