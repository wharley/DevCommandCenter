# MCP real-service smoke tests

These tests exercise DCC-managed MCP projection through the production Claude
and Codex adapters against real services. They are intentionally separate from
the offline conformance suite, ignored by default, and never selected by normal
CI.

Status as of July 28, 2026: the harness and offline configuration tests are
implemented; no authenticated real-service run has been recorded yet.

The first targets are:

- Figma's official remote MCP endpoint, `https://mcp.figma.com/mcp`, using
  provider-native OAuth and a disposable design node;
- Garu's official command server, `@garuhq/mcp@0.17.0`, using a dedicated test
  account and an API key imported into the backend-only DCC credential-store
  port.

Official references:

- [Figma remote MCP installation](https://developers.figma.com/docs/figma-mcp-server/remote-server-installation/)
- [Figma MCP tools](https://developers.figma.com/docs/figma-mcp-server/tools-and-prompts/)
- [Garu MCP repository](https://github.com/Garu-Pagamentos/garu-mcp)

The Garu package is MIT-licensed and is not added to the DCC dependency graph,
vendored, installed, or downloaded by the default suite. `npx` may fetch and
execute the exact pinned version only after the dedicated execution
acknowledgement is supplied.

## What the harness proves

Each ignored test:

1. creates an isolated system-temporary workspace;
2. persists a trusted DCC-managed definition and global binding in temporary
   SQLite;
3. resolves the definition through the normal session projection path;
4. starts the production Claude or Codex adapter;
5. requires the DCC-owned runtime status to report the expected read-only tool;
6. allows exactly one matching DCC permission request and denies any other
   request;
7. requires the matching tool call to complete;
8. accepts only the fixed assistant success sentinel; and
9. cancels the provider session and removes the temporary workspace.

Tool arguments, tool results, provider deltas, design content, charge data, and
API keys are never copied into assertions, snapshots, error messages, or result
artifacts. Failures use fixed categories. The Garu secret is moved into
`SecretValue`, projected as `GARU_API_KEY`, and zeroized when its backend-only
copies are dropped. Temporary SQLite contains only an opaque credential
reference.

The harness does not create verification evidence and cannot weaken or replace
the offline conformance gate.

## Figma read-only smoke

Figma requires a supported client and an interactive OAuth grant. Authenticate
the exact provider using Figma's official flow before running the corresponding
test. Ensure there is no second active Figma MCP definition in that provider
during the smoke; the test must exercise the DCC-projected definition.

Use a disposable file with no customer or production content. Copy a URL for
one node. The harness accepts only:

- HTTPS URLs on `figma.com` or `www.figma.com`;
- `/design/` or legacy `/file/` paths with an alphanumeric file key; and
- a bounded `node-id`.

It rebuilds a canonical URL and discards the display-name path, fragments, and
every query parameter other than `node-id`. The only permitted tool is
`get_design_context`. Canvas writes, uploads, file creation, and Code Connect
mutations are never allowed.

Set the non-secret fixture URL:

```sh
export DCC_FIGMA_MCP_FIXTURE_URL='https://www.figma.com/design/FILE_KEY/Disposable?node-id=1-2'
```

Run Claude:

```sh
DCC_RUN_CLAUDE_FIGMA_MCP_SMOKE=1 \
  cargo test -p dcc-mcp-fixture --test real_service_smokes \
  authenticated_claude_figma_read_only_smoke -- --ignored --exact
```

Run Codex:

```sh
DCC_RUN_CODEX_FIGMA_MCP_SMOKE=1 \
  cargo test -p dcc-mcp-fixture --test real_service_smokes \
  authenticated_codex_figma_read_only_smoke -- --ignored --exact
```

Optional model overrides are `DCC_CLAUDE_MCP_SMOKE_MODEL` and
`DCC_CODEX_MCP_SMOKE_MODEL`.

Figma currently restricts the remote server to clients in its MCP catalog. An
OAuth, allowlisting, seat, plan, or rate-limit rejection is an adapter-specific
smoke result. It must not be reported as generic DCC MCP support or converted
into `verifiedBridge` evidence.

## Garu command smoke

Use a dedicated Garu test account whose accessible data may safely be read by
the selected model provider. This smoke calls only `list_charges` with the
smallest supported page size. It never permits charge creation, customer
mutation, payment processing, deletion, or refunds.

Resolve the absolute `npx` executable yourself:

```sh
export DCC_GARU_MCP_NPX="$(command -v npx)"
```

Load the API key into `DCC_GARU_MCP_API_KEY` with a local secret manager or a
silent shell prompt. Do not put the key in a command line, shell history,
`.env`, repository file, issue, test output, or CI configuration.

Confirm the dedicated account and the pinned third-party execution:

```sh
export DCC_GARU_MCP_DEDICATED_TEST_ACCOUNT=1
export DCC_GARU_MCP_ALLOW_PINNED_EXECUTION=I_UNDERSTAND_THIS_RUNS_PINNED_THIRD_PARTY_CODE
```

Run Claude:

```sh
DCC_RUN_CLAUDE_GARU_MCP_SMOKE=1 \
  cargo test -p dcc-mcp-fixture --test real_service_smokes \
  authenticated_claude_garu_read_only_smoke -- --ignored --exact
```

Run Codex:

```sh
DCC_RUN_CODEX_GARU_MCP_SMOKE=1 \
  cargo test -p dcc-mcp-fixture --test real_service_smokes \
  authenticated_codex_garu_read_only_smoke -- --ignored --exact
```

Unset the secret after either run:

```sh
unset DCC_GARU_MCP_API_KEY
```

## Fork and CI policy

- Never add these variables to public or fork-triggered CI.
- Never remove `#[ignore]` from a real-service smoke.
- Never replace the pinned package with `latest`, a range, or an
  environment-provided package name.
- Never make mutating tools part of this unattended harness.
- A future mutating smoke requires a separate disposable fixture, a separate
  test, and an exact action-specific confirmation sentinel.
- Do not publish raw provider logs when diagnosing a failure. Record only the
  target, provider, exact provider/package version, fixed failure category, and
  timestamp.

## Default offline check

This command compiles the real adapters and smoke harness, runs its
configuration tests, and performs no network access or third-party execution:

```sh
cargo test -p dcc-mcp-fixture --test real_service_smokes
```

Expected default result: three configuration tests pass and four authenticated
tests remain ignored.
