# ADR: External MCP integrations

- **Status:** Accepted
- **Date:** July 28, 2026
- **Scope:** DCC consuming external MCP servers

## Context

DCC already has a `dcc mcp` command that exposes DCC capabilities to external
clients. The integrations discussed here run in the opposite direction: a user
adds an external MCP server, such as Figma or a payment gateway, and DCC makes
its tools available to compatible provider runtimes.

Provider support is heterogeneous. Some providers load native configuration,
some accept MCP definitions through a runtime protocol, and others have no
reliable attachment mechanism. Parsing an MCP tool-call event is useful, but it
does not prove that DCC can attach a server, preserve approvals, observe tool
availability, or clean up the integration.

Because DCC is open source, the safety model must remain inspectable and useful
to forks without depending on a private control plane or hidden allowlist.

## Decision

### Canonical registry and provider bridges

DCC owns a provider-neutral registry of external MCP definitions and their
session, project, or global bindings. Provider adapters translate eligible
definitions into the provider's supported runtime mechanism.

The normal product flow selects a scope, not a provider matrix. DCC attaches a
definition only to adapters whose compatibility has been verified. Unsupported
providers are skipped with an explicit reason.

### Direct injection for the MVP

The MVP uses direct provider injection. It does not introduce a general DCC MCP
gateway. This preserves provider-native identity and OAuth behavior and avoids
claiming proxy support for protocol capabilities that have not been tested.

A gateway may be reconsidered after the direct bridges and tools-first
permission boundary are stable.

### Tools-first boundary

The first compatibility contract covers MCP initialization, tool discovery,
tool calls, approval behavior, lifecycle, and observable status. Resources,
prompts, sampling, elicitation, and generalized proxying are separate future
capabilities.

### Evidence-backed compatibility

Provider descriptors use the following support levels:

- `unsupported`: DCC has no reliable attachment path;
- `nativeConfig`: the provider may load its own configuration, but DCC does not
  own or verify attachment and lifecycle;
- `verifiedBridge`: a DCC bridge passed the shared end-to-end conformance suite.

The stable provider preset defaults to `unsupported`. Tool-event parsing and
provider stability do not raise this level. No provider receives
`verifiedBridge` until attach, tool visibility, approval, disable, removal, and
restart behavior are tested for the relevant provider version.

### Persistence and secrets

Definitions and bindings will be stored in local SQLite. Secret-bearing headers
and environment values will be represented by opaque references whose values
live in an operating-system credential store. Secret values must not be
returned to the renderer or written to project files, generated provider
configuration, logs, analytics, crash reports, or snapshots.

### Trust and ownership

Repository-discovered definitions are untrusted and read-only by default. A
command server cannot execute until the user approves the resolved executable,
arguments, working directory, environment key names, source identity, and a
fingerprint of security-relevant fields. Changing that fingerprint invalidates
trust.

The canonical fingerprint and activation rules are documented in the
[MCP definition trust model](./MCP_TRUST_MODEL.md).

DCC modifies and removes only projections it created. Disabling a definition
stops future DCC attachment while preserving it. Removing it cleans up
DCC-owned processes and projections; deleting credentials remains a separate,
explicit choice.

### Permissions

Server annotations are not authorization. Unknown tools default to **Ask**.
Allow and deny decisions are scoped to a server and tool. An adapter is not a
verified bridge unless DCC can preserve or intercept the approval boundary,
especially for destructive, financial, identity, deployment, and deletion
operations.

## Consequences

- A provider can honestly expose native MCP awareness without being presented
  as a DCC-managed integration.
- New providers do not gain MCP claims by inheriting a generic capability
  preset.
- Claude and Codex can keep using user-owned native configuration while their
  DCC bridges are built, but DCC will not describe those integrations as
  connected.
- The shared conformance harness becomes the promotion gate to
  `verifiedBridge`.
- Direct bridges require provider-specific maintenance and version checks.
- A provider without a safe attachment or approval mechanism remains
  unsupported even if its underlying product supports MCP in another surface.

## Verification gates

Before the first public release:

1. Offline command and URL fixture servers exercise the shared tools-first
   conformance suite.
2. Claude and Codex pass the same lifecycle and approval scenarios.
3. Untrusted imported commands cannot start without consent.
4. Logs, persistence, generated configuration, renderer state, and diagnostic
   exports contain no secret values.
5. Authenticated Figma and pinned command-server smoke tests remain opt-in for
   contributors and forks.

## Related documents

- [External MCP integrations roadmap](./MCP_INTEGRATIONS_ROADMAP.md)
- [MCP provider conformance contract](./MCP_PROVIDER_CONFORMANCE.md)
- [Claude MCP bridge](./CLAUDE_MCP_BRIDGE.md)
- [MCP credential store policy](./MCP_CREDENTIAL_STORE.md)
- [Integrated terminal and scope plan](./PLANO_TERMINAL_INTEGRADO_E_ESCOPO.md)
- [Security policy](../SECURITY.md)
