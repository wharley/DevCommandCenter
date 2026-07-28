# Cursor MCP bridge

The Cursor bridge is being delivered through the Agent Client Protocol (ACP).
DCC does not edit Cursor's user or project `mcp.json`, does not register an
extension, and does not infer support merely because Cursor can use MCP from
its own configuration.

Status as of July 28, 2026: the ACP projection payload and fail-closed runtime
contract are implemented and tested offline. The production Cursor adapter
still reports MCP as unsupported and receives no DCC MCP definitions. Approval
routing, tool visibility, runtime status, and the shared conformance harness
remain required before activation.

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

## Remaining activation gate

The production adapter may return a DCC MCP projection version only after it
can:

1. start `cursor-agent acp` without falling back to the generic `agent`
   executable;
2. send the prepared session request over the private stdin channel;
3. correlate ACP tool calls and `session/request_permission` requests with the
   DCC definition and exact tool;
4. enforce `Ask`, `Allow`, and `Deny` without trusting titles or MCP
   annotations;
5. publish a connected runtime snapshot only after tool visibility is proven;
6. cancel the session and its DCC-projected MCP children deterministically; and
7. pass the shared stdio and HTTP provider conformance harness.

Until then, Cursor stays honestly unsupported in the DCC integrations
compatibility view. Users may continue using Cursor-native MCP configuration,
which DCC neither owns nor modifies.
