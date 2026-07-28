# Claude MCP bridge

The Claude bridge projects DCC-owned external MCP definitions into the Claude
Agent SDK without editing Claude configuration files or treating inherited
servers as DCC-owned.

This document describes the first Phase 3 slice. The production registry and
scope resolver are intentionally not connected yet, so Claude remains
`nativeConfig` rather than `verifiedBridge`.

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

1. Rust validates a bounded list of DCC-owned server projections.
2. Only names under the `dcc-` namespace are accepted, then the Claude wire
   name receives a random per-session namespace to avoid colliding with an
   inherited user entry.
3. stdio arguments, environment names, URLs, header names, and credential
   values are checked for invalid or injection-shaped input.
4. Rust serializes one `configure_mcp` NDJSON message directly to the sidecar's
   stdin.
5. The serialized byte allocation is redacted from `Debug` and zeroized after
   the write.
6. The sidecar validates the message again and retains it only in process
   memory for the provider session.
7. Each Agent SDK query receives the DCC projection through `mcpServers` while
   `settingSources: ["user", "project", "local"]` continues loading
   provider-owned sources independently.

An empty DCC projection omits the `mcpServers` option. It does not clear,
replace, or inspect inherited servers.

stdio and HTTP projections set `alwaysLoad: true`. This gives the bounded
startup window documented by the SDK and makes connection status observable
before DCC treats tools as available.

## Status and failure boundary

The sidecar reads `mcpServerStatus()` and emits only bounded metadata for names
in the DCC projection:

- DCC definition ID and provider-local name;
- normalized connection status;
- bounded tool names.

Provider configuration, URLs, headers, environment values, descriptions, and
raw SDK errors are not forwarded. A thrown attach error becomes the fixed
message `DCC MCP attachment failed`. `failed` and `needs-auth` statuses fail the
turn closed; `pending` remains distinct because the SDK may still be connecting.

MCP tools are not added to `allowedTools`. Anthropic documents that MCP tools
require explicit permission unless specifically allowed, and that
`acceptEdits` does not auto-approve them. The existing `canUseTool` callback
therefore remains the DCC approval path. This must still pass the real provider
conformance suite before the bridge is promoted.

## Deliberate limitations of this slice

- The SQLite definition/binding resolver does not populate `SessionConfig`
  yet; production sessions currently send an empty DCC projection.
- Credential resolution from the OS store is not connected to session attach
  yet.
- The sidecar status event is not yet normalized into DCC runtime status.
- The installed SDK stdio configuration has no per-server `cwd` field. The
  resolver must reject definitions that require one unless a documented
  provider mechanism is added.
- Disable/remove refresh behavior and the full Claude conformance adapter
  remain pending.
- No `verifiedBridge` evidence is emitted by this slice.

These boundaries let the next cut connect registry scopes and credentials
without weakening provider ownership or exposing secrets.

## Offline checks

```sh
cargo test -p dcc-core -p dcc-providers
node --test sidecar/src/mcp-config.test.mjs
node --check sidecar/src/index.mjs
```

The tests use placeholder credentials, make no provider request, and require no
Claude account.
