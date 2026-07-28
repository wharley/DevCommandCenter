# Droid MCP bridge boundary

The DCC Droid MCP path targets Factory's long-lived `stream-jsonrpc` protocol.
It does not call `droid mcp add`, edit user or project `mcp.json`, change
organization policy, or touch Factory's OAuth keyring.

Status as of July 28, 2026: a backend-only initialization serializer is
implemented and tested offline against the public Factory protocol schema. The
production Droid adapter remains `Unsupported` and returns no DCC MCP
projection version. No Droid CLI is installed in the audit environment, and
the public permission contract does not identify the owning MCP server.

## Why stream JSON-RPC

Factory documents `droid exec --input-format stream-jsonrpc --output-format
stream-jsonrpc` as the low-level integration mode for long-lived custom
clients. It supports session initialization, server-to-client permission
requests, MCP management, MCP inventory, status notifications, and process
cleanup.

References:

- [Droid Exec headless protocol](https://docs.factory.ai/cli/droid-exec/overview)
- [Factory MCP configuration](https://docs.factory.ai/cli/configuration/mcp)
- [Factory CLI reference](https://docs.factory.ai/droid-cli/cli-reference)
- [Factory Droid TypeScript SDK](https://github.com/Factory-AI/droid-sdk-typescript)

The normal `droid exec --output-format stream-json` path used by DCC today is
one turn per process and does not expose the reverse request channel required
for DCC-owned approvals. It also launches at autonomy `medium`, where many
tool calls may be auto-approved. It is not reused for projected MCP sessions.

## Open-source boundary

The public `@factory/droid-sdk` repository is Apache-2.0 licensed. DCC does not
vendor that SDK or the proprietary Droid CLI. The scaffold implements only the
documented JSON-RPC interface in Rust and keeps Factory as an optional external
runtime installed by the user.

The Factory CLI packages are not bundled into DCC. This avoids adding a
closed-source binary or its update mechanism to the DCC open-source
distribution.

## Audited protocol contract

The serializer is pinned to the public SDK source inspected at commit:

```text
Factory Droid SDK 0.6.0
SDK source c35b42b12a043f9f10053e854ff0d9306d2d60e9
Factory API version 1.0.0
Factory protocol version 1.51.0
protocol evidence droid-sdk@0.6.0+factory-protocol@1.51.0
```

Every prepared request contains:

```text
jsonrpc: 2.0
factoryApiVersion: 1.0.0
factoryProtocolVersion: 1.51.0
type: request
method: droid.initialize_session
```

Protocol version evidence is not Droid CLI identity. The SDK repository
explicitly warns that it is undergoing an overhaul and may lag active CLI
development. No CLI version is promoted from npm metadata or documentation
alone.

Factory standalone installations also auto-update by default. A future exact
runtime bridge must launch with `FACTORY_DROID_AUTO_UPDATE_ENABLED=false` and
verify `droid -v` before sending MCP commands or credentials.

## Private initialization scaffold

For a future eligible runtime, the inert builder:

1. validates the absolute workspace and stdio executable paths;
2. validates bounded server, argument, environment, header, URL, request ID,
   and explicit tool-policy inputs;
3. rejects per-server stdio working directories because the public session
   schema represents only the session `cwd`;
4. replaces DCC display names with random per-session wire names;
5. serializes stdio and Streamable HTTP definitions only in the
   `droid.initialize_session` request;
6. serializes credentials inside a redacted, zeroizing one-shot payload;
7. starts with `interactionMode: "auto"`, `autonomyLevel: "off"`, and
   `skipPermissionsUnsafe: false`; and
8. retains definition ownership and explicit `Allow`/`Deny` policy maps only
   in DCC backend memory.

`Ask` is rejected while the production adapter cannot complete and correlate
the reverse permission request. The builder is deliberately disconnected from
`DroidProvider`, so it never sends credentials to an unidentified runtime.

## Provider-owned configuration

Factory loads MCP configuration from user, ancestor-folder, project, and
organization-managed sources. User configuration takes precedence over folder
and project entries, and OAuth tokens are global.

Consequently:

- DCC must not use `droid mcp add`, which writes user configuration;
- DCC must not use `droid.add_mcp_server` until its persistence and ownership
  semantics are proven by an exact runtime;
- catalog or status presence is not proof that DCC owns a server;
- random DCC wire names must be matched through structured fields; and
- disabling or removing a DCC integration must never change provider-owned
  entries or global OAuth state.

The initial `mcpServers` field is the preferred candidate because it is part of
session initialization. Exact-runtime testing must still prove that the CLI
does not copy those definitions into persistent user configuration.

## Strong lifecycle evidence

The public SDK protocol exposes:

- `droid.list_mcp_servers`, with server name, status, source, type, and tool
  count;
- `droid.list_mcp_tools`, with structured `serverName`, tool name, enabled
  state, and optional schema;
- `mcp_status_changed`, `mcp_auth_required`, and `mcp_auth_completed`
  notifications;
- server and per-tool toggles; and
- `droid.close_session` for deterministic session cleanup.

These contracts are enough to design observed-only inventory and lifecycle
tracking without parsing titles or forwarding raw provider errors.

## Approval blocker

The public SDK `0.6.0` `droid.request_permission` schema represents each MCP
request as:

```text
confirmationType: mcp_tool
details.type: mcp_tool
details.toolName: <string>
details.impactLevel: <string>
toolUse.id/name/input
```

It does not include `serverName`. A tool name, even if it appears qualified,
cannot be split or compared by convention because Droid may also load user,
project, organization, plugin, or other provider-owned tools.

The scaffold includes a fail-closed predicate that rejects this public shape
as ownership evidence. Only an explicit structured server and tool identity
from an exact runtime may cross the future approval boundary.

## Activation gate

The production adapter may expose a DCC MCP projection version only after:

1. an exact Droid CLI version is installed, auto-update is disabled, and its
   `-v` output is pinned;
2. its response envelopes prove the audited Factory protocol version;
3. session initialization proves DCC definitions remain session-owned and do
   not persist into Factory configuration;
4. permission requests carry the exact MCP server and tool structurally;
5. DCC responds only with `proceed_once` or `cancel`, never persistent or
   auto-run outcomes;
6. status and inventory are filtered by exact random DCC wire names;
7. closing the session deterministically stops projected stdio children; and
8. the shared stdio and HTTP conformance harness passes.

Until then, users may continue using Droid-native MCP configuration, which DCC
neither owns nor modifies.
