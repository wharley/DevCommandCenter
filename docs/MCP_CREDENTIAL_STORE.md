# MCP credential store

MCP integrations store credential values in the operating-system credential
store. SQLite and renderer-facing contracts contain only opaque
`McpSecretReferenceId` values.

## Platform backends

DCC uses the default native backends from
[`keyring` 4.1.5](https://docs.rs/crate/keyring/4.1.5):

- macOS: Keychain;
- Windows: Credential Manager;
- Linux and other supported Unix desktops: Secret Service through zbus.

The dependency is used through its small `v1` API. The optional `cli` feature
is not enabled. The crate is dual licensed MIT or Apache-2.0.

## Linux fallback policy

DCC does not fall back to plaintext, a repository file, SQLite, an environment
variable, or an application-managed encryption key when Secret Service is not
available.

On a headless Linux environment without an unlocked compatible store, MCP
credential operations fail closed as `Unavailable` or `AccessDenied`. A future
encrypted-file fallback would require a separate threat model, an independent
key source, explicit user consent, and its own ADR.

## Security boundary

- The OS entry service is `com.devcommandcenter.app.mcp`.
- Entry usernames are opaque credential reference IDs.
- Secret bytes are capped, redacted from `Debug`, and zeroized on drop.
- Backend errors are converted to bounded categories without forwarding raw
  platform errors or malformed credential bytes.
- Resolve is backend-only and is not part of the Specta/Tauri renderer
  contract.
- Store is create-or-replace.
- Delete is explicit and independent from disabling or removing an MCP
  definition.
- Default tests use an in-memory fake and never touch a contributor's native
  credential store.

## Deliberately deferred

- Renderer commands for storing credentials.
- OAuth token acquisition and refresh.
- Resolving references into MCP transports.
- An opt-in native credential-store smoke test in release CI.
