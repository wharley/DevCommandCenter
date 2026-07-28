# Claude MCP bridge

The Claude bridge projects DCC-owned external MCP definitions into the Claude
Agent SDK without editing Claude configuration files or treating inherited
servers as DCC-owned.

The production session path now resolves DCC registry scopes and OS-backed
credentials for adapters that explicitly declare a projection channel. Claude
is currently the only such adapter. It remains `nativeConfig` rather than
`verifiedBridge` until the complete provider conformance suite passes.

## Documented SDK path

DCC uses the Agent SDK's programmatic `query({ options: { mcpServers } })`
contract. Anthropic documents stdio and HTTP configurations, environment and
header authentication, `alwaysLoad`, tool naming, status inspection, and the
independent `.mcp.json`/`settingSources` path in the
[Agent SDK MCP guide](https://code.claude.com/docs/en/agent-sdk/mcp).

The bundled versions for this slice are:

- `@anthropic-ai/claude-agent-sdk` `0.2.126`;
- `@anthropic-ai/claude-code` `2.1.126`.

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

The Claude adapter declares the exact runtime key
`claude-agent-sdk@0.2.126+claude-code@2.1.126`. A test binds that key to the
pinned package dependencies, so a dependency upgrade cannot silently retain
old runtime identity.

Rust validates the snapshot again and creates `McpRuntimeStatus` values bound
to the definition, provider, exact runtime version, and session. The backend
atomically replaces the in-memory snapshot and publishes
`dcc/session/mcp/runtime-status`. These values are deliberately not appended
to the durable session transcript. The snapshot is cleared when the provider
runtime ends or is cancelled, preventing a stale `Connected` state.

MCP tools are not added to `allowedTools`. Anthropic documents that MCP tools
require explicit permission unless specifically allowed, and that
`acceptEdits` does not auto-approve them. The existing `canUseTool` callback
therefore remains the DCC approval path. This must still pass the real provider
conformance suite before the bridge is promoted.

## Deliberate limitations of this slice

- The installed SDK stdio configuration has no per-server `cwd` field. The
  Claude adapter rejects definitions that require one instead of silently
  dropping it.
- Disable/remove refresh behavior and the full Claude conformance adapter
  remain pending.
- No `verifiedBridge` evidence is emitted by this slice.

These boundaries keep the bridge unadvertised as verified while the next cut
drives approval, lifecycle, and both fixture transports through the shared
provider conformance harness.

## Offline checks

```sh
cargo test -p dcc-core -p dcc-providers -p dcc-tauri
node --test sidecar/src/mcp-config.test.mjs
node --check sidecar/src/index.mjs
```

The tests use placeholder credentials, make no provider request, and require no
Claude account.
