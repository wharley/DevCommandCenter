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

The bundled integration dependencies are:

- `@anthropic-ai/claude-agent-sdk` `0.2.126`.

Claude Code itself is an external prerequisite installed and authenticated by
the user. The previous bundled baseline was `2.1.258`; it is not a pinned DCC
dependency anymore.

### External CLI validation — 2026-09-08

- The local MCP release gate passed, including Rust tests, contracts/desktop
  type checks and frontend MCP checks. All 43 sidecar tests passed; targeted
  regressions additionally cover CLI discovery, health and missing-install
  session rejection.
- The compiled Bun sidecar discovered the user's native Claude **2.1.259** from
  a temporary directory with only `/usr/bin:/bin` in PATH. Authentication passed
  with normal macOS Keychain access.
- `authenticated_claude_bridge_passes_the_shared_harness` passed against that
  compiled sidecar and external CLI, covering **stdio and HTTP**. No bundled CLI
  or source-script fallback was used. The fixture's text observer was updated to
  consume current `AssistantMessageDelta`/`AssistantMessageCompleted` events and
  reconcile final snapshots; its tool/content/permission assertions remain.
- Regression checks preserve inherited PATH precedence and put the user's
  native installation ahead of additional legacy NVM directories on GUI launches.
  Authentication probes check the documented command introduction version
  (2.1.41) before calling `auth status`, because old CLIs can interpret unknown
  subcommands as model prompts. They do not parse human-readable help output.

This is integration evidence for the tested SDK/CLI combination, not a claim of
compatibility with every CLI version. The next signed release artifact still
needs bundle validation and installed/download size measurement.

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

DCC signs remote connectors in itself (Settings → Connectors) and no provider
runs an OAuth login of its own. A connector with a DCC sign-in reaches every
provider as an HTTP server on DCC's loopback gateway
(`/mcp/connectors/<id>`), authenticated with the session lease; the gateway
adds the vendor token, refreshes it once on a 401 and never hands the token to
the agent. A remote server that asks for a sign-in DCC does not hold is left
out of the turn and shows Reconnect in Settings and the composer.

The earlier Claude path, a session-local `mcp-remote` stdio proxy whose tokens
were captured into the credential store, is removed. At startup DCC deletes
the grants that path stored and marks those connectors as needing a reconnect.

The Agent SDK can report a projected server as `pending` while its OAuth
callback and tool discovery are still completing. For queries with DCC-owned
MCP servers, the sidecar starts the provider process with an async prompt stream
but withholds the user message until every projected server leaves `pending`.
It publishes changed status snapshots during that wait (bounded at 30 seconds).
A server that fails, asks for a sign-in or times out is reported and left out;
the prompt is then released, so a broken integration never holds the turn.
This keeps the first model turn from taking a tool snapshot before an
attaching server is available.

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
The sidecar polls `pending` for at most the bounded OAuth attachment window and
converts an expired wait into a fail-closed provider attachment status.
For connected tools, the sidecar keeps only a bounded name and boolean
`readOnly`, `destructive`, and `openWorld` annotations exposed by the pinned SDK,
mapping them to the provider-neutral hint contract. Free-form annotation data
is discarded. Missing booleans remain unknown rather than receiving inferred
defaults.

The Claude adapter declares the projection key
`claude-agent-sdk@0.2.126+claude-code@external`. A test binds the SDK portion to
the pinned SDK dependency. Claude Code is installed and authenticated by the user;
DCC does not bundle, install, update or fall back to a private copy of the CLI.
The sidecar's `--resolve-cli` probe reports the actual executable path and version.
Before a session starts, Rust probes that executable and passes its resolved path
to the interactive sidecar. Missing or broken installations fail with setup
guidance. Health checks also require authentication via `claude auth login`.

Rust validates the snapshot again and creates `McpRuntimeStatus` values bound
to the definition, provider, SDK/external-CLI projection key, and session. The backend
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
```

The tests use placeholder credentials, make no provider request, and require no
Claude account.
