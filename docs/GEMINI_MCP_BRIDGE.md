# Gemini MCP bridge boundary

The DCC Gemini MCP path targets Gemini CLI's ACP server. It does not edit
Gemini's user `~/.gemini/settings.json`, project `.gemini/settings.json`, MCP
enablement file, OAuth token files, extensions, or policy files.

Status as of July 28, 2026: an exact-version, backend-only ACP projection
builder is implemented and tested offline for stdio and Streamable HTTP. The
production Gemini adapter remains `Unsupported` and returns no DCC MCP
projection version because Gemini's ACP permission events do not expose enough
structured ownership to mediate approvals safely.

## Why ACP

Gemini CLI documents ACP as its programmatic integration mode. ACP
`session/new` accepts session-owned `mcpServers`, including stdio, SSE, and
HTTP definitions. This gives DCC a private, non-persistent injection path and
avoids temporary edits to provider-owned configuration.

References:

- [Gemini CLI ACP mode](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/acp-mode.md)
- [Gemini CLI MCP servers](https://github.com/google-gemini/gemini-cli/blob/main/docs/tools/mcp-server.md)
- [ACP session setup](https://agentclientprotocol.com/protocol/v1/session-setup)
- [ACP tool calls and permission requests](https://agentclientprotocol.com/protocol/v1/tool-calls)

## Audited runtime contract

The current contract is pinned to:

```text
gemini-cli 0.32.1
ACP protocol 1
launch flag --experimental-acp
runtime evidence gemini-cli@0.32.1+experimental-acp-v1
```

The projection builder requires the ACP `initialize` response to report:

- `protocolVersion: 1`;
- `agentInfo.name: "gemini-cli"`;
- `agentInfo.version: "0.32.1"`; and
- `agentCapabilities.mcpCapabilities.http: true` when any Streamable HTTP
  definition is present.

No upgrade or downgrade inherits compatibility. A future version using the
documented `--acp` spelling needs a new audit and its own exact runtime
evidence.

## Private projection

For an eligible session, the builder:

1. validates absolute workspace, executable, policy, and additional-root
   paths;
2. validates bounded server, argument, environment, header, URL, and explicit
   tool-policy inputs;
3. replaces DCC display names with random per-session wire names;
4. serializes credentials only inside a redacted, zeroizing one-shot
   `session/new` payload;
5. returns an exact launch allowlist containing only those random wire names;
   and
6. retains definition ownership and explicit `Allow`/`Deny` policy maps only
   in backend memory.

The future runtime must launch with:

```text
--experimental-acp
--approval-mode default
--allowed-tools ""  # neutralize the deprecated user settings allowlist
--policy <absolute DCC-owned policy path>
--allowed-mcp-server-names <exact random DCC wire name>  # repeated
```

Additional workspace roots use repeated `--include-directories` arguments.
Secrets never appear in argv. The MCP allowlist is mandatory because Gemini
merges ACP-projected servers with servers discovered from its normal settings
and extensions; only the random DCC names may be allowed to connect in a
DCC-owned projected session. The empty legacy tool allowlist and DCC-owned
policy path prevent normal user-level auto-allow rules from silently bypassing
the DCC approval boundary. System administrator policy remains authoritative
and may deny the projection.

Per-server stdio working directories are rejected because ACP represents only
the session working directory. Silently substituting a different directory
would change the integration's meaning.

## Approval blocker

In Gemini CLI `0.32.1`, a confirmation-producing tool call sends
`session/request_permission` with:

- a tool-call ID;
- a human-readable title;
- generic status and kind;
- optional content and locations; and
- permission choices.

It does not send the MCP server name or the registry tool name. On that branch,
it also requests permission before publishing an `in_progress` tool-call
notification that DCC could correlate.

The official Gemini CLI main-branch implementation inspected on July 28, 2026
has the same ownership gap. Consequently, DCC cannot prove whether a request
belongs to:

- one of its projected MCP servers;
- a provider-native tool;
- a user-configured MCP server; or
- an extension tool.

Titles, descriptions, sanitized names, collision prefixes, and guessed naming
conventions are not authorization evidence. DCC also cannot use `trust: true`,
YOLO mode, or an allow-all policy: each would bypass the per-tool `Ask`,
`Allow`, and `Deny` boundary.

Unknown or ambiguous ownership must remain denied. This is why the safe
projection builder is intentionally not connected to the production adapter.

## Local protocol observation

With an isolated `GEMINI_CLI_HOME` and no model prompt, the installed CLI
successfully answered ACP `initialize` with protocol 1, agent identity
`gemini-cli 0.32.1`, and HTTP/SSE MCP capabilities.

This is compatibility observation, not conformance evidence. It does not prove
tool discovery, invocation, approvals, disable/remove behavior, or process
lifecycle.

## Activation gate

The production adapter may expose a DCC MCP projection version only after an
exact Gemini runtime can:

1. identify the MCP server and exact tool in structured permission or
   lifecycle data;
2. correlate the identity to a DCC-owned random wire name without titles or
   suffix heuristics;
3. enforce `Ask`, `Allow`, and `Deny` with one-call scope;
4. publish provider-reported tool inventory without arguments, results, or
   secrets;
5. isolate DCC servers from provider/user/extension configuration;
6. stop projected MCP children deterministically; and
7. pass the shared stdio and HTTP conformance harness.

Until then, users may continue using Gemini-native MCP configuration, which
DCC neither owns nor modifies.
