# Security Policy

## Supported versions

Security fixes target the latest public release and the current `main` branch.
Older releases may receive a mitigation when practical, but reporters should
not assume that an older build remains supported.

## Scope

This project includes:

- a local desktop application built with Tauri;
- local terminal execution and repository access;
- optional local HTTP access;
- optional mobile pairing flows; and
- external MCP integrations that may start local commands, connect to remote
  services, resolve credentials, and expose tools to model providers.

Security reports are especially valuable for issues involving:

- command execution boundaries;
- local HTTP authentication and authorization;
- mobile pairing flows;
- secret handling;
- workspace and filesystem isolation;
- activation of repository-provided MCP commands without informed consent;
- MCP command, argument, environment, working-directory, or URL validation
  bypasses;
- credentials appearing in SQLite, configuration files, command lines, logs,
  diagnostics, renderer state, snapshots, telemetry, or provider transcripts;
- OAuth grants or credentials being confused across definitions, projects,
  sessions, providers, or users;
- an MCP tool bypassing `Ask` or `Deny`, receiving persistent approval from a
  one-call decision, or being attributed to the wrong server;
- DCC deleting or modifying provider-owned MCP configuration;
- reserved-header replacement, Streamable HTTP session confusion, or unsafe
  remote endpoint handling;
- an MCP child process surviving disable, removal, session cancellation, or
  application shutdown; and
- unpinned or unacknowledged third-party command execution.

## Reporting a vulnerability

Do not open public GitHub issues for suspected vulnerabilities.

The preferred channel is GitHub private vulnerability reporting through the
repository's **Security** tab and **Report a vulnerability** button. Repository
administrators must enable that GitHub setting before treating this channel as
available.

If the button is unavailable, open the
[security contact request](https://github.com/wharley/DevCommandCenter/issues/new?template=security_contact.yml)
with no vulnerability details, proof of concept, logs, paths, provider names,
or credentials. The public issue should request only a private contact channel.

Once a private channel is established, include:

- the affected DCC version or commit;
- operating system and provider/runtime version when relevant;
- a short description and impact assessment;
- minimal steps to reproduce; and
- a minimal proof of concept when it can be shared safely.

Never send live credentials, tokens, certificates, OAuth grants, customer data,
private repository contents, complete environment dumps, raw MCP payloads, or
unredacted provider transcripts. Replace them with disposable values and
describe only the minimum structure needed to reproduce the issue.

For an MCP approval bypass, report the definition ownership, transport kind,
scope, expected policy, observed behavior, and whether a tool executed. Do not
include real tool arguments or results. For a command-execution issue, use a
harmless disposable command that demonstrates the boundary without reading or
modifying unrelated data.

## Response expectations

The project will try to:

- acknowledge the report;
- reproduce and validate the issue;
- prepare a fix or mitigation;
- coordinate disclosure after a fix is available; and
- credit the reporter when requested and appropriate.

Do not publish vulnerability details before a coordinated disclosure. A
provider rejecting an unsupported MCP integration, an authenticated smoke
requiring quota, or a provider being honestly marked unsupported is normally a
compatibility issue, not a vulnerability.

## Operational notes

- Treat local HTTP, pairing, terminal, provider, and MCP command surfaces as
  privileged.
- Prefer the default local-only configuration over exposing the backend to a
  wider network.
- If remote access is enabled, use explicit authentication and encrypted
  transport.
- Do not activate repository-provided MCP commands that you have not reviewed.
- Disabling or removing a DCC integration does not authorize DCC to delete a
  provider-owned definition.
- Revoke temporary OAuth grants and delete disposable test credentials after
  security validation.

The external MCP threat model and trust boundary are documented in
[MCP definition trust model](docs/MCP_TRUST_MODEL.md) and
[MCP release validation](docs/MCP_RELEASE_VALIDATION.md).
