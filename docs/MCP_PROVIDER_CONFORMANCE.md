# MCP provider conformance

The shared provider conformance harness is the promotion gate from provider
native MCP awareness to a DCC-managed `verifiedBridge`.

The harness lives in `dcc-core`, is provider-neutral, and runs the same
behavioral contract for stdio and Streamable HTTP. It does not require a
provider account in the default test suite.

## Version 1 contract

For each transport, an adapter must drive the real provider surface through
these behaviors:

1. reset adapter-owned test state and attach the repository fixture;
2. create a provider session and observe both `fixture.echo` and
   `fixture.mutate`;
3. call the read-only echo tool and receive the deterministic value;
4. request the mutating tool, observe an approval boundary, deny it, and prove
   that the fixture mutation did not execute;
5. disable the fixture and prove it is unavailable to a refreshed or new
   session;
6. remove it and confirm adapter-owned state is cleaned up;
7. make the server unavailable and observe a fail-closed result;
8. make its credential unavailable and observe a fail-closed result.

The harness requests idempotent final cleanup for each transport even when a
scenario fails, so interrupted CI runs do not intentionally retain
adapter-owned fixture state.

The application runner is `run_provider_mcp_conformance`. Provider-specific
test adapters implement `McpConformanceAdapter` and translate each stable step
into calls against their real runtime protocol. Returning the expected enum
without exercising that runtime is not conformance.

The in-memory fake in the core test suite verifies harness sequencing and
failure behavior only. It does not verify any production provider and cannot
promote Claude, Codex, or another adapter.

## Promotion evidence

A successful run creates `McpConformanceEvidence` containing only:

- provider ID and exact provider version;
- conformance suite and fixture contract versions;
- the two tested transport kinds and fixed check names;
- verification time.

It contains no tool arguments, tool results, provider transcript, stderr,
environment values, headers, credential references, or secret values.
Adapter errors are categorical for the same reason.

The Rust evidence fields are private and their normal constructor is restricted
to the harness. Consequently, constructing
`McpSupportLevel::VerifiedBridge` in adapter code requires evidence returned by
the shared run. Evidence loaded from persistence must pass `validate()` before
it is advertised.

The TypeScript contract represents verified support as an object containing
the evidence, while `unsupported` and `nativeConfig` remain explicit strings.
The UI shows the verified badge only for the evidence-bearing shape.

Because DCC is open source, a fork can intentionally modify these rules. The
goal is an inspectable, enforceable upstream contract rather than a private
attestation service or hidden allowlist.

## Versioning and compatibility

Current identifiers:

- suite: `dcc-mcp-provider-conformance-v1`;
- fixture: `dcc-mcp-fixture-v1`.

Changing required behavior creates a new suite version. Changing fixture
semantics that affect conformance creates a new fixture version. Evidence with
unknown versions or incomplete transport/check coverage is rejected.

Provider-version evidence is exact. A provider upgrade must run its bridge
suite again before that version can be advertised as verified. Claude and
Codex now share a production-provider conformance driver. Each bridge must
still pass its account-backed opt-in gate at the exact audited runtime before
it can claim full conformance.

## Provider opt-in gates

The provider driver for this harness lives in
`crates/dcc-mcp-fixture/tests/provider_conformance.rs`. It drives either the
production `ClaudeSdkSidecarAdapter` or `CodexAppServerAdapter`, the repository
fixture binary, the provider's normal permission callback, and the shared
harness rather than returning synthetic success observations.

The adapter covers both fixture transports and checks:

- provider-visible tool discovery from the normalized runtime snapshot;
- a real read-only `fixture.echo` call;
- a real `fixture.mutate` permission request followed by denial, with no
  completed mutating tool call;
- refreshed-session behavior after the DCC projection is disabled;
- provider-session and fixture cleanup after removal;
- categorical failure after the stdio process or HTTP endpoint is unavailable;
- credential resolution failure before any secret-bearing configuration
  reaches the selected provider.

The authenticated tests are ignored by default. Claude requires an existing
Claude Code login and `DCC_RUN_CLAUDE_MCP_CONFORMANCE=1`; Codex requires an
existing Codex login, the exact audited `codex-cli 0.145.0`, and
`DCC_RUN_CODEX_MCP_CONFORMANCE=1`. Optional model overrides use
`DCC_CLAUDE_CONFORMANCE_MODEL` and `DCC_CODEX_CONFORMANCE_MODEL`. None of these
variables carries a credential.

Run each gate only against its pinned runtime:

```sh
DCC_RUN_CLAUDE_MCP_CONFORMANCE=1 \
  cargo test -p dcc-mcp-fixture --test provider_conformance \
  authenticated_claude_bridge_passes_the_shared_harness -- --ignored --exact

DCC_RUN_CODEX_MCP_CONFORMANCE=1 \
  cargo test -p dcc-mcp-fixture --test provider_conformance \
  authenticated_codex_bridge_passes_the_shared_harness -- --ignored --exact
```

Each test uses an isolated system-temporary workspace and loopback fixture. It
may consume provider quota. Its evidence remains process-local; DCC continues
to advertise both providers as `nativeConfig` until the corresponding opt-in
gate is executed successfully and the promotion change is reviewed.

## Running the offline gate

From the repository root:

```sh
cargo test -p dcc-core mcp_conformance
cargo test -p dcc-mcp-fixture conformance_contract_names
cargo test -p dcc-mcp-fixture --test provider_conformance
node --test sidecar/src/mcp-config.test.mjs sidecar/src/permission-bridge.test.mjs
```

These default commands run without external network access, credentials, or
provider accounts. The ignored authenticated Claude test is not selected.

Real-service Figma and pinned command-server checks are a separate layer. Their
ignored harness, exact opt-in gates, read-only constraints, and fork policy are
documented in [MCP real-service smoke tests](MCP_REAL_SERVICE_SMOKES.md). A
real-service result cannot replace the shared offline provider conformance
contract or create promotion evidence by itself.

The [Cursor MCP bridge](CURSOR_MCP_BRIDGE.md) has an exact-version hybrid ACP
runtime, fail-closed approval correlation, and observed-only tool status. Its
authenticated shared-harness test is ignored by default. Cursor remains
`NativeConfig` and is not eligible for `VerifiedBridge` evidence until that
gate passes both transports and the full lifecycle.

The [Gemini MCP bridge](GEMINI_MCP_BRIDGE.md) has an inert, exact-version ACP
projection builder but no production projection channel. Gemini ACP permission
requests do not currently identify the MCP server and exact tool
structurally, so DCC cannot mediate per-tool policy without heuristics. Gemini
therefore remains `Unsupported` and is not eligible for conformance evidence.

The [Grok MCP bridge](GROK_MCP_BRIDGE.md) also has an inert, exact-version ACP
projection builder. Grok `0.2.101` reports HTTP/SSE MCP capability through ACP,
but the exact installed runtime has not proven the structured ownership and
reverse permission round trip required by DCC. Grok also imports provider-owned
MCP configuration, so catalog presence cannot establish ownership. Grok
therefore remains `Unsupported` and is not eligible for conformance evidence.

The [Droid MCP bridge](DROID_MCP_BRIDGE.md) has an inert serializer pinned to
the public Factory protocol schema, not to a Droid CLI runtime. Its lifecycle
and inventory contracts are structured, but the public MCP permission request
does not include the owning server name. No exact Droid runtime was available
for local audit. Droid therefore remains `Unsupported` and is not eligible for
conformance evidence.
