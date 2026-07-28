# Codex MCP bridge

The Codex bridge projects DCC-owned MCP definitions into a single app-server
thread. It does not edit Codex configuration files, replace native MCP entries,
or change the configured `CODEX_HOME`.

## Audited runtime contract

The first supported runtime is:

```text
codex-cli@0.145.0+app-server-protocol-v2
```

DCC probes `codex --version` before advertising its private projection channel
and requires the exact audited CLI version. During app-server initialization it
also verifies the version reported in `userAgent`. A missing, malformed, older,
or newer version disables DCC projection while leaving the provider's native
configuration behavior available.

This exact-version allowlist is intentional. A Codex upgrade must be checked
against the generated app-server schema and the offline conformance suite before
its version is added. DCC does not infer compatibility from a nearby version.

## Session-only projection

Eligible session, project, and global bindings are resolved by the DCC backend.
Their stdio or Streamable HTTP definitions are sent in:

```text
thread/start.params.config.mcp_servers
```

The projection travels over the app-server stdin JSON-RPC channel. DCC does not
pass MCP credentials in command-line arguments, persist them in a temporary
Codex home, or return them to the renderer. The one-shot serialized request is
redacted from debug output and its allocation is zeroized after writing.

Every projected server receives a random per-session `dcc-` wire name. This
prevents DCC definitions from overwriting or relying on a user-owned native
entry. Configuration is bounded and validated before it reaches Codex:

- at most 32 servers;
- at most 128 stdio arguments and 64 secret fields per server;
- no NULs in commands, arguments, paths, or environment values;
- valid environment names;
- valid HTTP header names and values that cannot replace transport-controlled
  headers;
- only `http` and `https` URLs without embedded credentials or fragments;
- no duplicate DCC-owned logical server names.

The app-server process is killed when its DCC session is dropped, and a failed
handshake explicitly starts process termination. Since the projection exists
only in the thread configuration, cleanup never deletes a user-created Codex
entry.

## Runtime status and tool lifecycle

After `thread/start`, DCC reads the documented `mcpServerStatus/list` inventory
with `toolsAndAuthOnly` detail. Pagination, cursor size, total entries, and tool
count are bounded. The result is normalized into a complete DCC snapshot:

- a listed DCC-owned server is `connected` with its sorted tool inventory;
- `notLoggedIn` becomes a fixed, bounded authentication failure;
- an attached server absent from the current inventory remains
  `attachingProvider`;
- malformed inventory becomes a fixed protocol failure without forwarding the
  provider payload.

DCC also consumes `mcpServer/startupStatus/updated`. `starting`, `ready`,
`failed`, and `cancelled` are mapped explicitly, including the documented
`reauthenticationRequired` reason. Startup events that arrive before the
`thread/start` response are retained by registering the random wire-name map
before the request is written. The latest complete snapshot is replayed when
the DCC event subscriber attaches, avoiding a startup race.

Only random names owned by the current DCC session are normalized. Native
user-configured Codex entries remain visible to Codex but are ignored by the
DCC runtime-status projection. Once a thread ID is known, notifications for
other threads are ignored.

The schema-backed `mcpToolCall` item lifecycle becomes the existing DCC
tool-started, tool-completed, and tool-failed events. Arguments, results, raw
provider errors, and random server names do not cross that event boundary.

## Remaining verification

Safe injection, explicit version gating, and runtime status normalization do
not yet constitute full Codex conformance. The remaining bridge work is:

1. route tool approvals through the DCC permission boundary;
2. run both offline fixture transports through the shared conformance harness;
3. verify direct and configured Codex homes during final end-to-end validation.

Until those gates pass, the public capability remains `nativeConfig`; the
backend-only projection version is internal wiring evidence, not a general
compatibility claim.
