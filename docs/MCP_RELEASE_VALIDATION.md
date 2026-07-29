# MCP release validation

This checklist is the final promotion path for the first public external MCP
integrations release. It separates deterministic contributor checks from
account-backed provider conformance and real-service smokes. Passing one layer
never substitutes for another.

Status as of July 28, 2026: the local gate and all opt-in harnesses are
implemented. No authenticated conformance or real-service result has been
recorded.

## Release decision

The release may proceed only when every required row is complete:

| Layer | Required for the first release | Current state |
| --- | --- | --- |
| Local MCP gate | Yes | Ready to run |
| Manual integrations lifecycle | Yes | Pending final validation |
| Claude shared conformance | Yes | Pending authenticated opt-in run |
| Codex shared conformance | Yes | Pending authenticated opt-in run |
| Figma read-only smoke | Yes, on at least one verified provider | Pending |
| Pinned command-based read-only smoke | Yes, on at least one verified provider | Current opt-in harness uses Garu; another reviewed target may satisfy the gate |
| Cursor shared conformance | No promotion without it | Pending |
| Gemini, Grok, and Droid | No; they must remain honestly unsupported | Blocked by documented protocol/runtime boundaries |
| MCP-scoped security and dependency review | Yes | Complete |

An optional-provider failure blocks promotion of that provider, not the release,
when the product continues to report its actual lower support level. A Claude
or Codex failure blocks the first release because both are required by the
definition of done.

## Layer 1 — Local contributor gate

Run from the repository root:

```sh
yarn test:mcp:release-local
```

The script:

- refuses to start when an authenticated conformance, real-service execution,
  or Garu secret variable is present;
- formats-checks the Rust workspace;
- tests the MCP domain, infrastructure, providers, fixture, Tauri commands,
  Claude sidecar, contracts, and integrations UI;
- selects no ignored tests and sends no model prompt;
- does not execute Figma, Garu, OAuth, or an MCP package downloaded at runtime.

It assumes repository dependencies are already installed. Package managers may
resolve missing build dependencies according to their normal configuration;
the tests themselves do not require provider accounts or real services.

Record only the commit, operating system, Rust/Node/Yarn versions, timestamp,
and pass/fail result. Do not attach environment dumps or raw provider logs.

## Layer 2 — Manual lifecycle

Use a disposable project and definitions with no production credentials or
customer data. Verify through the packaged or release-candidate desktop build:

1. create one URL definition and one harmless absolute-command definition;
2. review trust details before activating the command;
3. bind each definition at session, project, and global scope in separate
   checks;
4. verify provider compatibility is version-aware and does not imply that
   every installed provider supports projection;
5. start a new eligible session and observe only DCC-owned status and tool
   inventory;
6. set one discovered tool to each of `Ask`, `Allow`, and `Deny`, then verify
   the approval boundary on a disposable fixture;
7. disable a definition and verify a new or refreshed session cannot use it;
8. remove it while retaining its credential, then separately delete the
   credential;
9. restart DCC and verify removed or disabled definitions do not reattach; and
10. verify provider-owned MCP configuration was not edited or deleted.

Do not use a real payment, production Figma file, or mutating third-party tool
for this layer. DCC-owned process controls are not a release claim while
provider runtimes do not expose independently owned process handles.

## Layer 3 — Authenticated provider conformance

Run one provider at a time from a clean shell. Authenticate through the
provider's official flow first. These tests can consume model quota and send
the fixed harness prompts documented in
[MCP provider conformance](MCP_PROVIDER_CONFORMANCE.md).

Claude:

```sh
DCC_RUN_CLAUDE_MCP_CONFORMANCE=1 \
  cargo test -p dcc-mcp-fixture --test provider_conformance \
  authenticated_claude_bridge_passes_the_shared_harness -- --ignored --exact
```

Codex, recording the detected CLI and negotiated projection version:

```sh
DCC_RUN_CODEX_MCP_CONFORMANCE=1 \
  cargo test -p dcc-mcp-fixture --test provider_conformance \
  authenticated_codex_bridge_passes_the_shared_harness -- --ignored --exact
```

Cursor is a separate promotion gate, not a substitute for Claude or Codex:

```sh
DCC_RUN_CURSOR_MCP_CONFORMANCE=1 \
  cargo test -p dcc-mcp-fixture --test provider_conformance \
  authenticated_cursor_bridge_passes_the_shared_harness -- --ignored --exact
```

Record the exact provider projection version and the categorical outcome.
Never preserve prompts, tool arguments, tool results, credentials, provider
transcripts, or raw stderr as release artifacts. A successful result is valid
only for the runtime recorded in that evidence. Nearby versions may negotiate
the same contract, but do not inherit full conformance evidence.

Gemini, Grok, and Droid have no authenticated command in this checklist. Their
current ownership or runtime blockers are documented in their bridge documents,
and attempting to work around those blockers with name matching would violate
the release gate.

## Layer 4 — Real-service smokes

Follow [MCP real-service smoke tests](MCP_REAL_SERVICE_SMOKES.md). Run only:

- Figma `get_design_context` against a disposable node with no customer data;
- Garu `list_charges` against a dedicated test account, with the exact pinned
  package and explicit third-party execution acknowledgement.

At least one verified provider must pass each target. Running both Claude and
Codex is recommended for coverage but does not replace their shared conformance
gates. A provider allowlist, OAuth, plan, seat, or rate-limit rejection is
recorded as a target/provider-specific result and never as generic MCP
conformance evidence.

Unset `DCC_GARU_MCP_API_KEY` immediately after the Garu run. Never place it in
shell history, `.env`, CI configuration, screenshots, issues, or artifacts.

## Layer 5 — Open source release review

Before promotion:

- follow the scoped findings and remaining actions in
  [MCP open source review](MCP_OPEN_SOURCE_REVIEW.md);
- verify all real-service and authenticated tests remain `#[ignore]`;
- verify no fork- or pull-request-triggered workflow receives MCP credentials;
- review new dependency licenses and notices, including pinned smoke-test
  package metadata;
- review security-reporting guidance for MCP command execution, OAuth,
  credentials, and approval bypasses;
- verify presets, if added later, are data-only and link to official sources;
- confirm diagnostics and renderer contracts contain no secret values; and
- publish only the bounded result record below.

Optional repository-wide hardening and compliance follow-ups are recorded
separately in [MCP open source review](MCP_OPEN_SOURCE_REVIEW.md). GitHub private
vulnerability reporting and review of pre-existing Anthropic packages are not
requirements introduced by MCP and do not block this gate.

## Bounded result record

Use one row per run:

```text
commit:
layer:
target:
exact_runtime_or_package:
operating_system:
timestamp_utc:
result: pass | fail
failure_category:
reviewer:
```

`failure_category` must be one fixed, non-sensitive category such as
`configuration`, `authentication`, `attachment`, `permission_boundary`,
`tool_execution`, `response_contract`, `timeout`, or `cleanup`. Do not add raw
payloads or logs.

## Cleanup

After the final session:

1. stop every test provider session;
2. stop the local fixture process;
3. remove disposable workspaces and test definitions;
4. revoke temporary OAuth grants that are no longer needed;
5. delete dedicated test credentials from the OS credential store;
6. unset smoke variables; and
7. confirm normal provider-owned MCP definitions still exist unchanged.

If cleanup cannot be proven, record the gate as failed and investigate before
another authenticated run.
