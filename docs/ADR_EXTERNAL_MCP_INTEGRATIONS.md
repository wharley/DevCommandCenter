# ADR: External MCP integrations

- **Status:** Accepted
- **Date:** July 28, 2026; amended July 29 and July 30, 2026
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

The July 29 amendment permits one narrow adapter-local exception: when a
runtime accepts stdio MCP servers but exposes no programmatic OAuth-start
operation for remote HTTPS servers, its DCC adapter may translate that one
remote definition through a pinned, bundled stdio-to-HTTP OAuth transport. The
projection must remain session-owned, preserve the original DCC definition and
tool-policy identity, bind callbacks to loopback, keep secrets out of arguments
and renderer state, and clean up its plaintext session projection with the
provider session. A DCC-owned OAuth grant may survive in the OS credential
store behind provider, definition, and resource-bound metadata. This is not a
general shared DCC gateway and does not change other providers' direct
attachment paths. *Removed in October 2026: the connector gateway below
replaced this exception.*

### October 2026 amendment: DCC-owned sign-in and connector gateway

Provider-owned OAuth broke down for remote connectors: the Claude path signed
in only inside a turn, the Codex path only after a failed attach, and grants
kept inside a provider's config directory do not follow the user across
multiple provider accounts. DCC therefore signs in to remote HTTP connectors
itself and serves them to providers through a narrow gateway.

- **Sign-in.** DCC runs the MCP authorization flow outside any turn: an
  unauthenticated probe, RFC 9728 and RFC 8414/OpenID discovery (with the
  well-known fallbacks), client identification (the user's own app, a Client
  ID Metadata Document, or dynamic registration), PKCE S256 with `state` and
  `resource`, and an RFC 9207 `iss` check before redeeming the code. The
  callback listens on `127.0.0.1` only while a sign-in is pending.
- **Storage.** The grant is a versioned credential-store envelope under the
  `dcc` owner in the existing grant table. Refreshes are serialized per
  connector, because some vendors issue single-use refresh tokens.
- **Gateway.** A signed-in connector is not injected into the provider with
  its vendor token. The provider receives a `dcc-<slug>` HTTP server on DCC's
  existing loopback listener (`/mcp/connectors/<definition>`), authenticated
  with the same per-session lease bearer as DCC's own server. The gateway
  checks that the lease was granted the connector, swaps in a fresh vendor
  token, forwards each JSON-RPC exchange, and returns plain JSON. It refreshes
  and retries once on 401, and never answers 401 itself: a provider would
  start OAuth against DCC. A lost sign-in becomes a JSON-RPC error the model
  can relay plus a `mcp-connector-needs-auth` event for the UI.
- **Scope of the exception.** Only HTTP definitions with a DCC-owned grant use
  the gateway. Stdio and header-secret servers keep their direct paths. Server-initiated messages in streamed
  answers are dropped; resources, prompts, sampling, and elicitation remain
  out of scope.
- **No provider login.** The Claude `mcp-remote` bridge and the Codex
  in-turn login are removed. Grants they stored are deleted at startup and
  their connectors are marked as needing a reconnect. A remote server that
  asks for a sign-in DCC does not hold is left out of the turn.

- **The user's own config.** DCC reads, never writes, the remote servers in
  Claude Code (`.claude.json`), Codex (`config.toml`) and Cursor (`mcp.json`).
  It offers once to import them as connectors, and it does not send a provider
  a DCC connector that provider already loads from its own file (same URL):
  that provider keeps its own copy, every other provider gets DCC's.
- **The user's own app.** Servers that register no clients (Slack) sign in
  with an app the user creates; its redirect is
  `http://localhost:46327/oauth/mcp/callback`, served on both loopback
  addresses.

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

Remote OAuth grants follow the same boundary. SQLite stores only the
definition, the `dcc` owner, resource fingerprint, timestamps, and opaque
credential reference. Access tokens, refresh tokens, and dynamic client
registration secrets live together in a versioned credential-store envelope.
Provider-native grants remain owned by the provider and are never copied into
DCC storage.

### Turn preflight

*Amended October 2026.* A user's MCP integration never holds the turn. Before
the turn is recorded DCC attaches the provider session and, for runtimes that
report MCP status (`interactivePreflight`), waits a bounded time for the
projected servers to settle. A server that fails or asks for a sign-in is
left out of that turn; a lost sign-in is recorded so Settings and the
composer offer Reconnect. No browser opens from inside a turn, and the
renderer sends the turn in one call.

The earlier contract (open the provider's authorization URL, wait for the
definition to connect, then send) is removed together with provider-owned
sign-in.

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

Server annotations are not authorization. A tool without an explicit
decision follows the task's permission mode, like the provider's own tools: it
asks in supervised modes and runs in a full-access turn (October 2026; before
that, every such tool asked even under full access). Plan and read-only turns
never auto-run it. Allow and deny decisions are scoped to a server and tool and
always win over the task's mode. An adapter is not a
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
