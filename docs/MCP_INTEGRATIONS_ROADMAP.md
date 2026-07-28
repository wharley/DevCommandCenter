# Roadmap: External MCP Integrations

## Goal

Allow a user to connect external MCP servers such as Figma, payment gateways,
issue trackers, and observability tools once in DCC and use them from every
provider runtime that DCC has verified as compatible.

The intended path is:

```text
URL or command
      ↓
DCC validates the MCP server
      ↓
scope: session / project / global
      ↓
verified provider adapter attaches it to a session
      ↓
tools are visible, permissioned, observable, and removable
```

This roadmap covers DCC **consuming external MCP servers**. It is separate from
the existing `dcc mcp` command, where DCC acts as a server and exposes its own
worktree, process, pane, and diff primitives to external agents.

## Product principles

- **Open and generic:** URL and command-based servers are first-class. A
  curated catalog may simplify known integrations later, but it cannot be the
  only installation path.
- **Provider-neutral domain:** an MCP definition belongs to DCC. Claude,
  Codex, Cursor, and future runtimes are adapters.
- **Automatic by default:** the main flow does not ask the user to choose
  providers. DCC attaches the server to every compatible provider in the
  selected scope.
- **Compatibility is earned:** a provider is compatible only after its adapter
  passes the MCP conformance suite. A capability boolean is not evidence.
- **No misleading success:** `configured`, `reachable`, `attached`, and
  `tools visible` are different states. Only the last successful end-to-end
  state is shown as connected.
- **Safe lifecycle:** disabling stops future attachment without losing the
  definition. Removing stops DCC-owned processes, removes DCC-owned
  projections, and optionally deletes DCC-owned credentials.
- **User ownership is preserved:** DCC never silently edits or deletes an MCP
  configuration it did not create.
- **Local-first:** definitions, trust decisions, runtime status, and secrets
  remain local unless a future, explicit sharing feature says otherwise.
- **Tools first:** the MVP targets MCP tools. Resources, prompts, sampling,
  elicitation, and a general MCP gateway are separate capability slices.

## Open source trust model

DCC is open source and can be forked, modified, redistributed, and used against
untrusted repositories. The feature must therefore be safe without relying on
a private backend, a hidden allowlist, or unpublished security logic.

### Repository configuration is untrusted input

A cloned repository can contain an MCP definition whose `command` executes
arbitrary code. DCC must not start project-discovered command servers merely
because a file exists in a checkout or worktree.

- Import and inspection are read-only by default.
- Starting an imported command requires explicit trust for the resolved
  executable, arguments, working directory, and definition fingerprint.
- A material definition change invalidates the previous trust decision.
- Each worktree inherits the project's decision only when the definition
  fingerprint still matches.
- URL servers discovered in a repository also require explicit activation;
  they can exfiltrate context even when they do not execute locally.
- Trust is never inferred from Git authorship, branch name, forge, or repository
  popularity.

### Command and dependency supply chain

- Show the exact command, arguments, working directory, and environment key
  names before first execution. Secret values remain masked.
- Warn on floating package versions such as `@latest`, unpinned Git references,
  and shell indirection.
- Prefer direct executable plus argument arrays. Do not invoke a shell merely
  to interpret a user-provided command string.
- Resolve the executable deterministically and show the resolved path.
- Never auto-update an MCP package or command.
- Bound stdout, stderr, startup time, restart attempts, and process count.
- Stop DCC-owned child processes on disable, removal, and app shutdown.

### Remote server and OAuth boundaries

- Accept only explicit `http` and `https` URLs in the initial implementation.
- Reject credentials embedded in URLs.
- Do not follow a redirect to a different origin without explicit consent.
- Treat loopback, LAN, and public endpoints as distinct destinations in the
  confirmation UI.
- OAuth callbacks bind to loopback, use state and PKCE where supported, expire
  quickly, and cannot reuse mobile-pairing credentials.
- Headers that carry credentials are stored as secret references, not plain
  definition fields.
- Network access by an MCP server is never described as sandboxed unless DCC
  actually enforces that boundary.

### Secrets, privacy, and diagnostics

- Secret values live in an OS credential store behind a DCC abstraction.
- SQLite, project files, generated provider configuration, logs, analytics,
  crash reports, and test snapshots contain only opaque secret references.
- Tool arguments and results may contain source code, customer data, payment
  data, or credentials. Persist bounded metadata by default, not full payloads.
- Diagnostic export is opt-in, bounded, and redacted.
- MCP tool names, server status, and coarse error categories may be recorded;
  sensitive arguments and results are not telemetry.
- Removing a definition and deleting its credential are separate choices.

### Permission boundary

Server annotations such as read-only or destructive hints are useful input but
not trusted authorization.

- Unknown tools default to **Ask**.
- A provider adapter is fully supported only if DCC can preserve or intercept
  the required approval boundary.
- Policies are `Ask`, `Allow`, or `Deny`, scoped to a server and tool.
- Destructive, financial, identity, deployment, and data-deletion operations
  never become globally allowed from server metadata alone.
- Production credentials can trigger a stronger warning, but DCC must not
  attempt to infer an environment from an arbitrary secret format.

## Technical identity

The domain should use DCC terminology and remain separate from vendor config
formats.

### Canonical definition

Illustrative domain shape:

```text
McpDefinition
├── id
├── displayName
├── transport
│   ├── Stdio { executable, args, cwd }
│   └── Http { url }
├── secretRefs
├── enabled
├── ownership
│   ├── DccManaged
│   └── ImportedReadOnly
├── trust
│   ├── currentFingerprint
│   └── decision
├── createdAt
└── updatedAt

McpBinding
├── definitionId
├── scope
│   ├── Session(sessionId)
│   ├── Project(projectId)
│   └── Global
├── enabled
└── optionalProviderExclusions
```

Provider exclusions are an advanced control for security, name collisions, or
runtime defects. The default remains every verified compatible provider.

Definitions and bindings are local DCC state. A future portable project
manifest may contain non-secret definitions and secret reference names, but it
is not required for the MVP.

### Runtime truth

Compatibility and connection status are derived, not persisted as permanent
truth:

```text
McpRuntimeStatus
├── definitionId
├── providerId
├── providerVersion
├── sessionId
├── state
│   ├── Disabled
│   ├── NeedsTrust
│   ├── ProbingServer
│   ├── ServerReachable
│   ├── AttachingProvider
│   ├── Connected
│   ├── Unsupported
│   └── Failed
├── tools
├── checkedAt
└── boundedError
```

`Connected` requires proof that the provider session can see the expected MCP
tools. A successful server handshake alone is `ServerReachable`, not
`Connected`.

### Provider contract

The current broad `Capabilities.mcp` flag must be replaced or backed by a
concrete contract. An illustrative adapter boundary:

```text
McpProviderBridge
├── support(runtimeVersion)
│   ├── transports
│   ├── scopes
│   ├── protocolFeatures
│   ├── canReportStatus
│   └── canEnforceApprovals
├── attach(session, resolvedDefinitions)
├── inspect(session)
├── detach(session, definitionIds)
└── normalizeError(error)
```

The bridge belongs beside each provider adapter. It translates DCC definitions
into SDK options, app-server configuration, ACP session parameters, or another
documented provider mechanism.

No generic fallback writes an unknown provider's config file. Unsupported
providers remain unsupported until their bridge is implemented and tested.

### Server probe versus provider verification

These are independent operations:

1. **Server probe**
   - resolve or start the transport;
   - perform MCP initialization and capability negotiation;
   - list tools;
   - classify authentication and protocol errors;
   - terminate the probe cleanly.
2. **Provider verification**
   - attach through the provider's bridge;
   - start or restart the scoped session as required;
   - inspect provider-reported MCP status or tool inventory;
   - verify tool visibility;
   - preserve provider approval handling.

When a runtime cannot report its MCP status or tool inventory, DCC may show
`Configured — not verified` in diagnostics, but that runtime does not receive a
public `Verified` compatibility badge.

### Scope semantics

- **Session:** attached only to one DCC session and removed when that session
  ends. This requires runtime injection; temporary edits to global user config
  are not an acceptable implementation.
- **Project:** selected for every new compatible session whose DCC project ID
  matches. Worktree paths do not create duplicate definitions.
- **Global:** selected for every new compatible local session.

Changing scope or disabling a definition affects new turns only after the
provider has confirmed a live refresh. Otherwise DCC restarts or asks to
restart the affected session and states that clearly.

## Phase 0 — Architecture decision and truth cleanup

### Deliverable 0.1 — ADR

Decision record: [ADR: External MCP integrations](./ADR_EXTERNAL_MCP_INTEGRATIONS.md).

Record the following decisions:

- canonical DCC registry with provider bridges;
- direct provider injection for the MVP;
- no general MCP gateway in the MVP;
- tools-first capability boundary;
- automatic use by all verified providers in scope;
- local SQLite definitions plus OS credential store references;
- imported configurations are read-only and untrusted by default.

Direct injection is preferred initially because some remote services authorize
or allowlist specific clients and because provider-native OAuth flows should
remain intact. A DCC gateway can be evaluated later for servers that allow it.

### Deliverable 0.2 — Honest capabilities

- Audit every provider descriptor that currently exposes `mcp: true`.
- Separate event parsing support from server attachment support.
- Remove or downgrade claims that are not backed by a bridge.
- Make provider version part of the compatibility result.
- Keep the existing ability to render MCP tool-call events; it is useful but
  does not prove attachment.

### Acceptance criteria

- Documentation distinguishes DCC-as-server from DCC-as-host.
- No provider is presented as MCP-compatible solely because it uses the stable
  capability preset.
- The ADR identifies the trust, secret, and permission boundaries.

## Phase 1 — Domain, persistence, and trust foundation

### Deliverable 1.1 — Domain model

- Add provider-neutral definitions, bindings, runtime status, and error types.
- Represent command and URL transports without shell strings.
- Model ownership and trust fingerprints explicitly.
- Keep provider exclusions optional and out of the primary flow.

### Deliverable 1.2 — Local persistence

- Persist definitions and project/global bindings in SQLite.
- Tie session bindings to DCC sessions without making ephemeral status durable
  truth.
- Add backward-compatible migrations and repository tests.
- Ensure exports and debug dumps omit secret values.

### Deliverable 1.3 — Credential store

Implementation policy:
[MCP credential store](./MCP_CREDENTIAL_STORE.md).

- Introduce a cross-platform credential-store port with macOS, Windows, and
  Linux implementations or an explicitly documented Linux fallback.
- Store opaque IDs in SQLite.
- Support create, replace, resolve, and delete with audit-safe errors.
- Never return a secret value to the renderer after storage.

### Deliverable 1.4 — Trust

Implementation policy:
[MCP definition trust model](./MCP_TRUST_MODEL.md).

- Compute a stable fingerprint over the executable or URL definition,
  arguments, cwd, environment key names, and source identity.
- Require explicit activation for imported definitions.
- Invalidate trust when security-relevant fields change.
- Preserve user-owned files on disable and removal.

### Acceptance criteria

- Definitions survive restart without persisting credentials in plain text.
- A cloned repository cannot start an MCP process without user confirmation.
- Removing a DCC definition does not modify an imported config.
- Unit tests use fake credential-store and persistence adapters.

## Phase 2 — MCP probe and conformance harness

### Deliverable 2.1 — Offline fixture server

Implementation:
[DCC offline MCP fixture](./MCP_OFFLINE_FIXTURE.md).

Add a small, repository-owned MCP fixture with deterministic behavior:

- `stdio` and Streamable HTTP modes;
- a read-only echo tool;
- a mutating tool that requires approval;
- tool-list change notification when supported;
- bounded slow, cancellation, malformed-result, and failure cases;
- no network or account dependency.

### Deliverable 2.2 — Probe service

Implementation policy:
[MCP probe service](./MCP_PROBE.md).

- Initialize and list tools for both MVP transports.
- Bound startup, response size, stderr, and shutdown.
- Normalize protocol, auth, executable, timeout, and transport failures.
- Redact secrets before errors cross the backend boundary.

### Deliverable 2.3 — Provider conformance suite

Implementation contract:
[MCP provider conformance](./MCP_PROVIDER_CONFORMANCE.md).

Every verified bridge runs the same behavioral tests:

1. attach the fixture;
2. create a provider session;
3. confirm tool visibility;
4. call the read-only tool;
5. receive an approval request for the mutating tool;
6. disable and confirm it is unavailable to a new or refreshed session;
7. remove and confirm cleanup;
8. fail closed when the server or credential is unavailable.

### Acceptance criteria

- Default tests are offline and require no provider account.
- Test output and snapshots contain no secrets or full sensitive payloads.
- A provider cannot gain a `Verified` badge without the conformance suite.

## Phase 3 — Claude bridge

Current implementation:
[Claude MCP bridge](./CLAUDE_MCP_BRIDGE.md).

### Deliverables

- Inject resolved definitions through documented Claude Agent SDK options.
- Preserve user, project, and local setting sources without treating inherited
  servers as DCC-owned.
- Normalize MCP status and tool-call events.
- Route tool approvals through the existing DCC permission flow.
- Support session, project, and global DCC scopes without editing user files.
- Handle refresh or clearly request a session restart.

### Acceptance criteria

- Both fixture transports pass the conformance suite.
- Imported Claude configuration remains untouched.
- Disable and remove have distinct, verified behavior.
- A Claude CLI or SDK regression downgrades runtime status instead of silently
  reporting success.

## Phase 4 — Codex bridge

### Deliverables

- Inject definitions through documented Codex app-server or startup
  configuration.
- Preserve configured `CODEX_HOME` and auth-overlay behavior.
- Use app-server MCP status and tool-call events when available.
- Route approvals through the DCC permission flow.
- Keep DCC-owned temporary configuration session-bound and recoverable after a
  crash.

### Acceptance criteria

- Both fixture transports pass the same conformance suite as Claude.
- Direct and configured Codex homes do not lose user settings or MCP logins.
- Cleanup cannot remove user-created config entries.
- Unsupported Codex versions are reported before a misleading connection
  attempt.

## Phase 5 — MVP user experience

### Deliverable 5.1 — Integrations surface

- List definitions with scope, transport, trust, and live status.
- Add a server by URL or executable plus arguments.
- Store secret fields without returning their values to the renderer.
- Show exact command and destination before activation.
- Test connection without permanently enabling a definition.

### Deliverable 5.2 — Lifecycle

- Enable and disable.
- Remove definition.
- Optionally delete DCC-owned credentials.
- Show sessions that require restart.
- Show DCC-owned processes and stop them deterministically.

### Deliverable 5.3 — Compatibility

The default copy is:

```text
Available in every verified provider in this scope
```

The details view shows:

```text
Claude Code   Connected
Codex         Connected
Cursor        Not verified
Grok          Unsupported by this runtime
```

Provider exclusions live under an advanced disclosure and are not required
during installation.

### Deliverable 5.4 — Tool policies

- Show the discovered tool inventory.
- Default unknown tools to `Ask`.
- Allow `Ask`, `Allow`, and `Deny` per tool.
- Make server annotations visible as hints, not policy.
- Record bounded approval metadata without arguments or results by default.

### Acceptance criteria

- A user can add one URL server and one command server without editing JSON.
- Scope is the only provider-related decision in the primary flow.
- Status distinguishes server reachability from provider connection.
- Disable, remove, and credential deletion are understandable and reversible
  where possible.
- No command copied from a repository runs before trust is granted.

## Phase 6 — Real integration smoke tests

Real services validate what the offline fixture cannot, but remain opt-in and
separate from the default suite.

### Figma

- Use the official remote server and provider-native OAuth.
- Validate Claude and Codex separately.
- Use a disposable design fixture and read-only operation first.
- Do not automate canvas writes in unattended tests.
- Treat client allowlisting or OAuth-client restrictions as an adapter result,
  not a generic MCP failure.

### Command-based API integration

- Use a dedicated test account and a pinned package version.
- Store the API key through the DCC credential store.
- Exercise a read-only tool and do not print its payload.
- Never run charge creation, refunds, deletion, deployment, or equivalent
  mutating operations in unattended smoke tests.

### Acceptance criteria

- Smokes are ignored by default and documented separately.
- CI forks do not receive integration credentials.
- Mutating smoke tests, if ever added, require a dedicated fixture and an
  exact confirmation sentinel.
- A failed real-service smoke cannot weaken offline conformance requirements.

## Phase 7 — Additional provider bridges

Recommended order:

1. Cursor through ACP;
2. Gemini;
3. Grok;
4. Droid;
5. other provider runtimes.

Each provider is an independent delivery slice:

- documented injection path;
- version-aware support probe;
- fixture conformance suite;
- approval-boundary review;
- enable, disable, removal, and restart behavior;
- one optional real-service smoke where useful.

There is no project-wide deadline that forces an unverified provider to claim
support.

## Phase 8 — Import, portability, and advanced capabilities

### Read-only import first

- Detect common project and user config locations.
- Mask credentials during inspection.
- Let the user import a definition into DCC explicitly.
- Preserve the source file and mark the imported record's ownership.
- Resolve conflicts by stable ID and source, never by server display name
  alone.

### Portable project manifest

Evaluate a versioned `.dcc` manifest containing:

- non-secret definitions;
- secret reference names;
- desired project scope;
- optional tool-policy suggestions.

Opening a repository never grants trust or resolves a secret automatically.

### Deferred MCP protocol capabilities

Add resources, prompts, subscriptions, sampling, and elicitation only after
their provider bridge and permission implications are understood. Capability
support is reported granularly.

### DCC MCP gateway

Evaluate a mediator where providers attach to one DCC endpoint and DCC connects
to downstream servers. This can centralize lifecycle and policy, but is not a
default migration because it can:

- interfere with provider-native OAuth;
- change the client identity seen by allowlisted services;
- expand DCC's protocol and security responsibilities;
- require correct proxying of capabilities beyond tools.

## Out of scope for the MVP

- A closed catalog or mandatory marketplace.
- Claiming support for every provider or every MCP protocol capability.
- Automatically running repository-provided MCP commands.
- Synchronizing secrets through Git or a DCC cloud service.
- Rewriting or deleting provider configuration that DCC does not own.
- Automatically allowing tools based only on their names or annotations.
- Automatic package updates.
- A generic MCP gateway.
- Team-wide policy administration.
- Full tool-argument and result retention.

## Implementation order

1. ADR and two-direction MCP terminology.
2. Capability truth cleanup.
3. Domain types and SQLite migrations.
4. Credential-store abstraction.
5. Trust fingerprints and ownership.
6. Offline MCP fixture and probe.
7. Provider conformance harness.
8. Claude bridge.
9. Codex bridge.
10. Integrations UI and lifecycle.
11. Tool policies and approval routing.
12. Figma and command-based real-service smokes.
13. Cursor bridge.
14. Remaining providers, one verified adapter at a time.
15. Read-only import and portable manifest evaluation.
16. Non-tool capabilities and gateway evaluation.

## MVP release gates

### Architecture gate

- ADR merged.
- Existing capability claims audited.
- DCC-as-server and external integrations use unambiguous product copy.

### Reliability gate

- Claude and Codex pass identical offline conformance tests.
- URL and command transports pass attach, use, disable, remove, and restart
  scenarios.
- Provider or server failure never appears as connected.

### Security gate

- Threat model reviewed publicly.
- No plaintext secrets in database, files, logs, telemetry, snapshots, or
  renderer state.
- Untrusted project config cannot execute without consent.
- Approval interception is verified for mutating tools.
- DCC-owned and user-owned cleanup paths are tested separately.

### Open source gate

- Contributors can run the default suite without external accounts.
- Authenticated smokes are opt-in and safe for forks.
- Dependency licenses and notices are reviewed.
- Presets, if present, are data-only and link to official sources.
- Security reporting documentation covers MCP commands, OAuth, secrets, and
  tool-approval bypasses.

### Product gate

- The primary flow asks for server details and scope, not a provider matrix.
- Compatibility details are evidence-backed and version-aware.
- A user can disable, remove, and separately delete credentials.
- The UI explains when an existing session must restart.

## Definition of done

The first public MCP integrations release is complete when:

- a user can add arbitrary URL and command definitions;
- session, project, and global scopes behave as documented;
- Claude and Codex are verified end to end;
- other providers are honestly shown as unverified or unsupported;
- tools and approval policies are visible;
- disable and removal are deterministic;
- project-provided commands require trust;
- secrets remain outside portable configuration and diagnostics;
- offline conformance tests protect the feature for contributors and forks;
- Figma OAuth and one pinned command-based integration have passed documented,
  opt-in smoke tests.

## Progress

Updated July 28, 2026:

- [x] Product and architecture discussion.
- [x] Current support and market-pattern review.
- [x] Roadmap and open source trust model.
- [x] Phase 0 ADR.
- [x] Capability truth cleanup.
  - [x] Replace the inherited MCP boolean with explicit support levels.
  - [x] Audit and downgrade current provider claims.
  - [x] Add provider-version evidence to bridge compatibility results.
- [x] Domain and credential-store foundation.
  - [x] Provider-neutral definitions, transports, bindings, trust, and runtime contracts.
  - [x] SQLite repositories and idempotent startup migrations.
  - [x] OS credential-store port and platform adapters.
  - [x] Fingerprint computation and activation service.
- [x] Offline fixture and conformance harness.
  - [x] Deterministic offline fixture for stdio and Streamable HTTP.
  - [x] Bounded probe service for both transports.
  - [x] Shared provider conformance harness.
- [ ] Claude bridge.
  - [x] Add the backend-only projection channel and documented Agent SDK injection.
  - [x] Resolve eligible session, project, and global bindings with OS credentials.
  - [x] Normalize SDK status into DCC runtime status.
  - [ ] Verify approval, lifecycle, and both fixture transports through the harness.
    - [x] Add the production-sidecar conformance adapter and compile it offline.
    - [x] Prove permission denial and missing-credential fail-closed behavior offline.
    - [ ] Run the authenticated opt-in gate during final end-to-end validation.
- [ ] Codex bridge.
  - [x] Add exact-version-gated, session-only projection through
    `thread/start.params.config.mcp_servers`.
  - [x] Preserve native `CODEX_HOME` and keep MCP credentials off argv,
    persistence, diagnostics, and renderer contracts.
  - [x] Normalize app-server startup, inventory, and tool-call status.
  - [x] Route one-call approvals through the DCC permission boundary with
    fail-closed ownership and lifecycle correlation.
  - [ ] Verify lifecycle and both fixture transports through the harness.
    - [x] Add the production app-server adapter to the shared conformance
      driver and compile it offline.
    - [x] Prove missing-credential fail-closed behavior for both transports.
    - [ ] Run the authenticated opt-in gate during final end-to-end validation.
- [ ] MVP integrations UI.
  - [x] Add renderer-safe list, create, activate, disable, and remove commands.
  - [x] Keep credential values write-only and make credential deletion explicit.
  - [x] Add the integrations surface and URL/command creation flow.
  - [x] Add trust review plus enable, disable, remove, and optional credential
    deletion controls.
  - [x] Add evidence-backed compatibility, live per-session runtime snapshots,
    and restart-required views.
  - [x] Add discovered-tool inventory plus persisted Ask, Allow, and Deny
    policies enforced by the Claude and Codex bridges.
  - [x] Show explicitly reported boolean tool annotations as untrusted hints
    without changing approval policy.
  - [ ] Add DCC-owned process controls after the runtime exposes independently
    owned MCP process handles.
- [ ] Real-service smoke tests.
  - [x] Add ignored, fail-closed Figma and pinned Garu read-only smoke harnesses
    plus fork-safe execution documentation.
  - [ ] Run the authenticated opt-in smokes during final end-to-end validation.
- [ ] Additional provider bridges.
  - [ ] Cursor through ACP.
    - [x] Add an exact-version and capability-gated ACP v1 projection builder
      for stdio and Streamable HTTP without modifying Cursor-owned config.
    - [x] Confirm the audited Cursor runtime starts and stops the offline stdio
      fixture through `session/new`, without a model turn.
    - [x] Route projected sessions through the production ACP adapter while
      preserving the existing adapter for sessions without DCC MCP servers.
    - [x] Correlate structured DCC-owned tool and permission events, enforce
      per-tool policy fail-closed, and report only observed tool inventory.
    - [ ] Run the shared authenticated conformance gate during final
      end-to-end validation.
  - [ ] Gemini through ACP.
    - [x] Add an exact-version and capability-gated ACP v1 projection builder
      for stdio and Streamable HTTP without modifying Gemini-owned config.
    - [x] Add random per-session server names, a launch-time MCP allowlist,
      redacted one-shot credentials, and isolated policy-path requirements.
    - [x] Confirm `gemini-cli 0.32.1` identity and MCP capabilities through an
      isolated no-model ACP initialization.
    - [x] Document the activation blocker: Gemini permission requests expose
      no structured MCP server/tool ownership and cannot safely implement
      `Ask`, `Allow`, and `Deny`.
    - [ ] Wire the production adapter only after an exact runtime exposes
      correlatable ownership, then run the shared authenticated conformance
      gate during final end-to-end validation.
  - [ ] Grok through ACP.
    - [x] Add an exact-version and capability-gated ACP v1 projection builder
      for stdio and Streamable HTTP without modifying Grok-owned or imported
      configuration.
    - [x] Add random per-session wire names, redacted one-shot credentials,
      and in-memory definition and policy ownership maps.
    - [x] Confirm `grok-build 0.2.101` identity and MCP capabilities through a
      no-model ACP initialization.
    - [x] Document coexistence with Grok-, Cursor-, Claude-, plugin-, and
      gateway-owned MCPs; catalog presence is not ownership evidence.
    - [x] Document the activation blocker: the exact installed runtime has not
      proven structured ownership through permission events, and the current
      DCC adapter cannot answer reverse permission requests.
    - [ ] Wire the production adapter only after the exact runtime proves that
      boundary, then run the shared authenticated conformance gate during
      final end-to-end validation.
  - [ ] Droid.
