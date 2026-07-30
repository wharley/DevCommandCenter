# Claude MCP bridge

The Claude bridge projects DCC-owned external MCP definitions into the Claude
Agent SDK without editing Claude configuration files or treating inherited
servers as DCC-owned.

The production session path now resolves DCC registry scopes and OS-backed
credentials for adapters that explicitly declare a projection channel. The
catalog exposes Claude's exact projection runtime as `runtimeBridge`; it does
not claim `verifiedBridge` without persisted, reviewed conformance evidence.

## Documented SDK path

DCC uses the Agent SDK's programmatic `query({ options: { mcpServers } })`
contract. Anthropic documents stdio and HTTP configurations, environment and
header authentication, `alwaysLoad`, tool naming, status inspection, and the
independent `.mcp.json`/`settingSources` path in the
[Agent SDK MCP guide](https://code.claude.com/docs/en/agent-sdk/mcp).

The bundled versions for this slice are:

- `@anthropic-ai/claude-agent-sdk` `0.2.126`;
- `@anthropic-ai/claude-code` `2.1.126`;
- `mcp-remote` `0.1.38`, pinned for the Claude remote-OAuth transport bridge.

The installed TypeScript contract also exposes `mcpServerStatus()` and defines
`mcpServers` as an in-memory query option. No provider file mutation is needed.

## Backend-only projection channel

`SessionConfig` can carry `ProviderMcpServerConfig` values only inside the Rust
backend. The field is skipped by Serde and Specta, and credential values use
`SecretValue`, whose `Debug` representation is redacted.

When a Claude sidecar starts:

1. Rust selects enabled and trusted definitions through applicable DCC
   bindings, then resolves their opaque references through the OS credential
   store.
2. The provider adapter validates a bounded list of DCC-owned server
   projections.
3. Only names under the `dcc-` namespace are accepted, then the Claude wire
   name receives a random per-session namespace to avoid colliding with an
   inherited user entry.
4. stdio arguments, environment names, URLs, header names, and credential
   values are checked for invalid or injection-shaped input.
5. Rust serializes one `configure_mcp` NDJSON message directly to the sidecar's
   stdin.
6. The serialized byte allocation is redacted from `Debug` and zeroized after
   the write.
7. The sidecar validates the message again and retains it only in process
   memory for the provider session.
8. Each Agent SDK query receives the DCC projection through `mcpServers` while
   `settingSources: ["user", "project", "local"]` continues loading
   provider-owned sources independently.

An empty DCC projection omits the `mcpServers` option. It does not clear,
replace, or inspect inherited servers.

stdio and HTTP projections set `alwaysLoad: true`. This gives the bounded
startup window documented by the SDK and makes connection status observable
before DCC treats tools as available.

### Remote HTTPS OAuth

The installed Claude Agent SDK can report `needs-auth`, but it does not expose
the interactive OAuth-start operation through its programmatic query surface.
For a DCC-owned remote HTTPS definition, the sidecar therefore projects an
adapter-local stdio transport backed by the pinned
[`mcp-remote`](https://github.com/geelen/mcp-remote) bridge. ClickUp documents
this bridge for Claude clients that need to connect to its remote MCP endpoint.

This remains one DCC definition. Codex receives the original HTTPS definition
and uses its native OAuth operation; Claude receives the session-local transport
projection. No provider configuration file or duplicate integration is
created.

The Claude OAuth bridge:

- launches only from the already bundled sidecar, never through `npx` or a
  floating package version;
- forces Streamable HTTP instead of transport fallback;
- binds the OAuth callback server to loopback and uses the upstream PKCE flow;
- passes resolved DCC headers through child-only environment variables, never
  command arguments or renderer state;
- suppresses proxy diagnostics that could contain authorization URLs or header
  metadata;
- stores OAuth registration, verifier, and token files only inside a
  mode-`0700` random temporary directory owned by the provider session; and
- recursively removes that exact directory when the Claude sidecar exits.

Tokens are intentionally ephemeral in this slice. They remain available across
turns of the same Claude session, but a new Claude provider session may require
browser authorization again. Persisting remote OAuth tokens will require an
OS-credential-store implementation rather than copying the proxy's plaintext
file format into application data.

`mcp-remote@0.1.38` also required a narrow pinned patch: after forwarding the
`initialize` response, the upstream proxy did not propagate the negotiated
protocol version to its HTTP transport. The postinstall patch sets that
version so subsequent requests carry `MCP-Protocol-Version`. It fails closed if
the pinned package layout or version changes, and the local strict HTTP fixture
exercises `initialize` followed by `tools/list`.

## Scope and credential resolution

The resolver is provider-neutral. A definition is selected once when at least
one enabled binding applies to the current session, its project, or the global
scope. Provider exclusions are binding-local: an excluded binding does not
cancel a second applicable binding for the same definition.

Selected definitions must also be enabled and have a trust decision matching
their current security fingerprint. Disabled and untrusted definitions are
omitted. A binding that references an unavailable definition is treated as
registry corruption and fails closed.

Credential references are resolved only in the backend through
`SystemCredentialStore`. Missing, locked, corrupt, or otherwise unavailable
credentials abort provider attachment with the fixed message
`MCP credential resolution failed`; definition names, credential references,
and secret values are not included. Resolved values remain in `SecretValue`
and `ProviderMcpSecret` allocations, both redacted from `Debug` and zeroized on
drop.

Adapter routing is an explicit code contract, not provider-name inference or a
runtime trial. Providers without a declared DCC projection channel receive an
empty projection and continue using only their native configuration.

## Status and failure boundary

The sidecar reads `mcpServerStatus()` and emits a complete, deterministic
snapshot for every name in the DCC projection. It includes only:

- DCC definition ID and provider-local name;
- normalized connection status;
- bounded tool names.

SDK states map into the provider-neutral runtime contract as follows:

- `connected` becomes `Connected`, with the bounded tool names visible to the
  provider session;
- `pending` becomes `AttachingProvider`;
- `disabled` becomes `Disabled`;
- `needs-auth` becomes `Failed` with the `Authentication` category and a fixed
  message;
- `failed` becomes `Failed` with the `Provider` category and a fixed message.

An expected server absent from the SDK response remains `AttachingProvider`;
it is never inferred to be connected. Duplicate or unknown SDK status entries
fail closed. If status inspection itself throws, the sidecar emits a failed
entry for every projected server before aborting the turn.

Provider configuration, URLs, headers, environment values, descriptions, and
raw SDK errors are not forwarded. A thrown attach error becomes the fixed
message `DCC MCP attachment failed`. `failed` and `needs-auth` statuses fail the
turn closed; `pending` remains distinct because the SDK may still be connecting.
For connected tools, the sidecar keeps only a bounded name and boolean
`readOnly`, `destructive`, and `openWorld` annotations exposed by the pinned SDK,
mapping them to the provider-neutral hint contract. Free-form annotation data
is discarded. Missing booleans remain unknown rather than receiving inferred
defaults.

The Claude adapter declares the exact runtime key
`claude-agent-sdk@0.2.126+claude-code@2.1.126`. A test binds that key to the
pinned package dependencies, so a dependency upgrade cannot silently retain
old runtime identity.

Rust validates the snapshot again and creates `McpRuntimeStatus` values bound
to the definition, provider, exact runtime version, and session. The backend
atomically replaces the in-memory snapshot and publishes
`dcc/session/mcp/runtime-status`. These values are deliberately not appended
to the durable session transcript. The snapshot is cleared when the provider
runtime ends or is cancelled, preventing a stale `Connected` state. The
interactive sidecar process is also marked for termination on Rust-handle drop,
covering interrupted tests and abnormal adapter teardown in addition to the
normal explicit cancel path.

Unknown MCP tools are absent from both `allowedTools` and `disallowedTools` and
therefore default to the existing DCC `canUseTool` approval path. Explicit
per-definition `Allow` and `Deny` overrides are projected to the exact random
session-qualified MCP tool name. The sidecar also validates the policy map and
applies it in `canUseTool` when Claude requests a callback, so native tools and
provider-owned MCP servers cannot inherit DCC policy by a display-name match.
Policy records contain only bounded definition IDs, tool names, decisions, and
timestamps; arguments and results never enter persistence or renderer state.
Tool annotations are displayed as untrusted server hints and never select or
change one of those decisions.

The callback now lives in a separately tested sidecar module. Offline tests
prove that MCP requests stay pending until DCC responds, that an explicit
denial returns `deny`, and that aborting a request also fails closed. The real
Claude conformance adapter additionally waits for the production
`PermissionRequested` event, denies `fixture.mutate`, and rejects the run if a
matching mutating tool call completes.

## Conformance gate

The repository now contains an opt-in `McpConformanceAdapter` for the production
Claude sidecar. It drives the same shared steps for stdio and Streamable HTTP:
status-based tool discovery, `fixture.echo`, mutation denial, disable, removal,
server failure, credential failure, and final cleanup.

The account-backed test remains ignored by default. It requires an existing
Claude Code login and the non-secret opt-in
`DCC_RUN_CLAUDE_MCP_CONFORMANCE=1`. This is intentional: a mocked provider
response cannot create `verifiedBridge` evidence.

On July 29, 2026, the complete shared harness passed for stdio and Streamable
HTTP against
`claude-agent-sdk@0.2.126+claude-code@2.1.126` after fixing terminal-result
ordering between consecutive turns and normalizing tool completion from the
SDK's actual `tool_result`. The final confirmation passed on commit `c1ccc0b`;
promotion still requires the separate reviewed product decision that persists
and advertises evidence.

## Deliberate limitations of this slice

- The installed SDK stdio configuration has no per-server `cwd` field. The
  Claude adapter rejects definitions that require one instead of silently
  dropping it.
- The installed Agent SDK reports `needs-auth` and can reconnect an MCP server,
  but its programmatic query surface does not expose the native interactive
  OAuth-start operation. Remote HTTPS definitions use the session-local bridge
  described above; other transport-specific native authorization mechanisms
  remain unsupported until they expose a safe runtime contract.
- No `verifiedBridge` evidence is persisted or advertised by this slice; the
  final promotion decision remains bound to a reviewed release-candidate
  record.

These boundaries keep the bridge unadvertised as verified while allowing every
contributor and fork to inspect and compile the exact promotion gate.

## Offline checks

```sh
cargo test -p dcc-core -p dcc-providers -p dcc-tauri
cargo test -p dcc-mcp-fixture --test provider_conformance
node --test sidecar/src/mcp-config.test.mjs sidecar/src/permission-bridge.test.mjs \
  sidecar/src/turn-lifecycle.test.mjs
node --check sidecar/src/index.mjs
node scripts/patch-mcp-remote.mjs
```

The tests use placeholder credentials, make no provider request, and require no
Claude account.
