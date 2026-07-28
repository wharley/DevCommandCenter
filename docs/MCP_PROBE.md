# MCP probe service

The MCP probe verifies that a trusted definition can initialize and expose a
valid tool list before any provider bridge is allowed to claim compatibility.
It is provider-neutral and does not persist runtime truth.

## Security gate

Probing is an active operation: `stdio` starts a process and HTTP contacts a
remote endpoint. Therefore a definition must be structurally valid and its
current fingerprint must already have an explicit matching trust decision.
Merely clicking “test connection” never bypasses trust.

An explicitly trusted definition may be probed while disabled. This allows a
future UI to test a connection without permanently enabling provider
attachment.

For command definitions, the current implementation additionally requires:

- an absolute, existing executable path;
- a canonical path on Unix, preventing symlink or `PATH` reinterpretation;
- an explicit, absolute, existing working directory;
- direct argument-array execution, without a shell;
- a cleared inherited environment, populated only by declared secret
  environment bindings.

Bare commands such as `npx` are intentionally rejected until the definition
preparation flow can resolve the executable, display that path, and include it
in the trusted definition. This is fail-closed behavior, not a compatibility
claim.

## Protocol flow

Both transports perform the same bounded sequence:

1. send `initialize` with stable protocol revision `2025-11-25`;
2. validate the negotiated stable revision and required response shape;
3. send `notifications/initialized`;
4. call `tools/list`;
5. validate and retain only bounded tool names;
6. close stdin or terminate the HTTP session.

The HTTP adapter supports either JSON or SSE responses to POST requests, as
required by
[Streamable HTTP](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports).
It carries a server-issued `MCP-Session-Id` when present and attempts a bounded
`DELETE` during cleanup.

## Default limits

| Boundary | Limit |
| --- | --- |
| Initialization | 5 seconds |
| Tool discovery | 5 seconds |
| Shutdown | 1 second |
| JSON-RPC response or SSE stream | 256 KiB |
| Retained stderr | 0 bytes; fully drained with an 8 KiB observation cap |
| Tool count | 256 |
| Tool name | 128 characters using MCP-safe ASCII characters |
| Unrelated stdio messages | 32 |
| Unrelated SSE events | 32 |

The stdio process receives EOF first. If it does not stop within the shutdown
limit, DCC kills it; on Unix the process is placed in a dedicated process group
so descendants are killed with it.

HTTP redirects and implicit system proxies are disabled. This prevents a
trusted destination and its secret headers from being silently redirected or
forwarded through an unreviewed intermediary. Proxy support, if added later,
must be explicit and trust-bound.

## Credentials and renderer boundary

SQLite contains only opaque credential references. Immediately before a probe,
the backend resolves each reference from the credential-store port:

- stdio secrets become only their declared environment variables;
- HTTP secrets become only their declared headers;
- transport-owned headers such as `Host`, `Content-Type`,
  `MCP-Protocol-Version`, and `MCP-Session-Id` cannot be replaced.

Missing, denied, corrupt, non-UTF-8 environment, or invalid header credentials
all become a fixed `authentication` error. Credential IDs, target names,
values, response bodies, stderr, and native error text never cross the probe
result boundary.

A successful report contains only:

- definition ID;
- transport kind;
- negotiated protocol revision;
- validated tool names;
- check timestamp.

Server descriptions, tool descriptions, arbitrary metadata, and full payloads
are discarded.

## Normalized failures

The probe maps failures into the existing MCP categories:

- `invalidDefinition`;
- `authentication`;
- `executableNotFound`;
- `timeout`;
- `protocol`;
- `transport`;
- `permissionBoundary`.

Messages are fixed, bounded, and audit-safe. Raw process, HTTP, JSON-RPC,
credential-store, and server error messages are not forwarded.

## Scope boundary

This service proves MCP-level reachability and tool discovery only. It does not
call tools, attach a provider, route permissions, mark a provider as
`verifiedBridge`, or persist a “connected” state. Those guarantees belong to
the shared provider conformance harness and provider adapters.
