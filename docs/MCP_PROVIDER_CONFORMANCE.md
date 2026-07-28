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
suite again before that version can be advertised as verified. Phase 3 and
Phase 4 will add real Claude and Codex adapter implementations to this harness.

## Running the offline gate

From the repository root:

```sh
cargo test -p dcc-core mcp_conformance
cargo test -p dcc-mcp-fixture conformance_contract_names
```

Both commands run without external network access, credentials, or provider
accounts.
