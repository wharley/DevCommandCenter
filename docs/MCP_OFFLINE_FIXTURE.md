# DCC offline MCP fixture

`dcc-mcp-fixture` is a repository-owned MCP server used by the probe and
provider conformance suites. It is deterministic, requires no provider
account, and makes no external network request.

The fixture targets the stable MCP revision `2025-11-25` and negotiates the
stable `2025-06-18` and `2025-03-26` revisions for compatibility testing. The
implementation follows the official MCP
[lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle),
[Streamable HTTP transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
and [tools contract](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).

## Running it

From the repository root:

```sh
cargo run -p dcc-mcp-fixture -- stdio
```

The stdio transport reads and writes newline-delimited JSON-RPC. Standard
output contains MCP messages only. Diagnostics go to standard error.

```sh
cargo run -p dcc-mcp-fixture -- http --bind 127.0.0.1:8765
```

The Streamable HTTP endpoint is `http://127.0.0.1:8765/mcp`. Port `0` may be
used by automated tests to request an ephemeral port; the selected endpoint is
printed to standard error.

The HTTP fixture:

- refuses non-loopback bind addresses;
- rejects non-loopback browser origins;
- limits request bodies to 64 KiB;
- limits concurrent JSON-RPC requests to 32;
- requires the Streamable HTTP `Accept` contract;
- requires a supported `MCP-Protocol-Version` after initialization;
- returns JSON for requests and `202 Accepted` for notifications;
- exposes server notifications through an SSE `GET /mcp` stream.

It intentionally has no authentication because it is a process-local test
fixture bound to loopback. It must not be deployed as a shared service.

## Deterministic tools

| Tool | Behavior | Security annotation |
| --- | --- | --- |
| `fixture.echo` | Returns a string of at most 4,096 characters. | Read-only, idempotent, closed world |
| `fixture.mutate` | Increments an in-memory counter. With `changeTools: true`, toggles `fixture.dynamic` and emits `notifications/tools/list_changed`. | Mutating, non-idempotent, closed world |
| `fixture.slow` | Completes after 0–2,000 ms and honors `notifications/cancelled`. | Read-only, idempotent, closed world |
| `fixture.fail` | Returns a deterministic tool execution error using `isError: true`. | Read-only |
| `fixture.malformed_result` | Returns valid JSON-RPC containing an intentionally invalid MCP `content` field. | Negative-test fixture |
| `fixture.dynamic` | Appears only after the mutating tool toggles the tool list. | Read-only |

Tool annotations are untrusted hints, not authorization. In particular, the
fixture does not assert that approval occurred before `fixture.mutate` runs.
The provider conformance harness must prove that the DCC/provider approval
boundary prevented an unapproved call.

Mutation affects memory only. The fixture never writes project files, invokes
commands, resolves credentials, or contacts another host.

## Failure and lifecycle coverage

The shared server core also handles:

- initialization and protocol negotiation;
- ping;
- deterministic tool ordering;
- unknown methods and tools;
- invalid JSON-RPC and malformed JSON;
- bounded inputs and delay;
- concurrent slow calls and cancellation;
- tool-list change broadcasts.

The malformed-result behavior is deliberately non-conformant and must only be
called by negative tests. Normal initialization, listing, echo, mutation,
failure, and cancellation responses remain valid JSON-RPC.

## Scope boundary

This fixture is Deliverable 2.1. It does not probe arbitrary MCP servers,
attach a provider, grant a compatibility badge, or route production tool
permissions. Those behaviors belong to the probe and provider conformance
slices.
