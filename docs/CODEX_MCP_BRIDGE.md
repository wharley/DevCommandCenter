# Codex MCP bridge

The Codex bridge projects DCC-owned MCP definitions into a single app-server
thread. It does not edit Codex configuration files, replace native MCP entries,
or change the configured `CODEX_HOME`.

## Runtime-negotiated contract

DCC probes `codex --version` before advertising its private projection channel
and derives a diagnostic identity such as:

```text
codex-cli@0.146.0+app-server-protocol-v2
```

The version is evidence metadata, not a compatibility allowlist. During
app-server initialization DCC requires a bounded, well-formed version in
`userAgent` that matches the executable it launched. It then exercises the
actual contract through `thread/start.params.config.mcp_servers` and
`mcpServerStatus/list`. A version change alone does not disable MCP projection;
a missing field, rejected request, malformed status, or lost permission
correlation fails closed.

The provider catalog reports this path as `runtimeBridge`, distinct from a
`verifiedBridge` backed by the complete authenticated conformance suite. The
selected session's live status is authoritative. A failed negotiation records a
bounded protocol error with the detected runtime identity rather than silently
falling back to a compatibility guess.

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

For a DCC-owned server reported as `notLoggedIn`, the selected session can
explicitly start Codex's native `mcpServer/oauth/login` flow. DCC resolves the
definition to its private wire name in the backend, requests an authorization
URL for the active thread, and exposes only a bounded HTTPS (or loopback HTTP)
URL after rejecting embedded credentials. When a pending user turn encounters
that status, DCC opens the URL as a preflight, waits for the matching server to
become connected, and then sends the original prompt without creating an
interrupted turn. The settings action remains an explicit recovery path. DCC
does not receive, persist, or render the resulting access or refresh token. A matching
`mcpServer/oauthLogin/completed` notification triggers a fresh status
inventory; notifications for unknown wire names or other threads are ignored.

The schema-backed `mcpToolCall` item lifecycle becomes the existing DCC
tool-started, tool-completed, and tool-failed events. Arguments, results, raw
provider errors, and random server names do not cross that event boundary.

## Tool approval boundary

Every DCC-projected server sets Codex
`default_tools_approval_mode = "prompt"`. Threads and turns that contain the
projection use the app-server's granular approval policy with only
`mcp_elicitations` enabled. Sandbox, rule, skill, and standalone permission
approvals remain disabled in this channel.

The negotiated app-server contract represents an MCP tool approval as the
documented
`mcpServer/elicitation/request` form whose `_meta.codex_approval_kind` is
`mcp_tool_call`. DCC accepts that request only when:

- its JSON-RPC ID, thread, server, turn, item, and tool fields are bounded;
- the random server name belongs to the current DCC projection;
- exactly one unclaimed active `mcpToolCall` item matches that server and turn;
- the request uses the audited MCP tool-approval metadata.

The prior `item/started` notification provides the tool identity. DCC does not
parse the provider-controlled elicitation message or forward tool arguments,
form content, metadata, or random server names into renderer events. An
unowned, malformed, out-of-thread, or ambiguous request is declined
automatically.

The existing DCC permission card receives an opaque DCC request ID. `allow`
maps to the app-server's one-call `accept`, while `deny` maps to `decline`.
Other behaviors are rejected. DCC intentionally does not expose Codex's
session or persistent approval hints yet, because those choices are not part
of the current provider-neutral permission contract. Turn cleanup, item
completion, process exit, and explicit cancellation clear pending requests as
denied or cancelled rather than allowing a tool to proceed.

Persisted per-definition tool policies are evaluated only after the same
owned-server, active-item, thread, and tool correlation succeeds. Unknown tools
default to `Ask`. An explicit `Allow` or `Deny` answers the one-call elicitation
directly and emits bounded request/resolution metadata without arguments,
results, provider form content, or random server names. Policies are captured
when the provider session starts, so changing one marks an existing session as
requiring restart instead of claiming a live update.

The status inventory retains only the bounded tool name and explicitly present
boolean `readOnlyHint`, `destructiveHint`, `idempotentHint`, and
`openWorldHint` values. Free-form annotation fields are discarded. These values
are renderer-visible hints only; they do not participate in approval
correlation or policy resolution.

## Conformance gate

The repository now compiles the production `CodexAppServerAdapter` against the
same provider-neutral conformance driver used by Claude. The driver covers
stdio and Streamable HTTP discovery, `fixture.echo`, explicit denial of
`fixture.mutate`, disable, removal, server failure, credential failure, and
final cleanup.

The default suite remains offline. It proves that missing credentials fail
before Codex attachment and compiles the real lifecycle and approval path, but
does not manufacture provider success or consume an account. The full gate is
ignored by default and requires the non-secret
`DCC_RUN_CODEX_MCP_CONFORMANCE=1` and an authenticated account:

```sh
DCC_RUN_CODEX_MCP_CONFORMANCE=1 \
  cargo test -p dcc-mcp-fixture --test provider_conformance \
  authenticated_codex_bridge_passes_the_shared_harness -- --ignored --exact
```

An optional model override can be provided through
`DCC_CODEX_CONFORMANCE_MODEL`. The authenticated execution, direct and
configured `CODEX_HOME` checks, and promotion decision remain deferred to the
final end-to-end validation.

Until that gate passes for a runtime, the public capability remains
`runtimeBridge`; it must not be presented as conformance-verified.
