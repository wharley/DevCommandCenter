# Codex MCP bridge

The Codex bridge projects DCC-owned MCP definitions into a single app-server
thread. It does not edit Codex configuration files, replace native MCP entries,
or change the configured `CODEX_HOME`.

## Audited runtime contract

The first supported runtime is:

```text
codex-cli@0.145.0+app-server-protocol-v2
```

DCC probes `codex --version` before advertising its private projection channel
and requires the exact audited CLI version. During app-server initialization it
also verifies the version reported in `userAgent`. A missing, malformed, older,
or newer version disables DCC projection while leaving the provider's native
configuration behavior available.

This exact-version allowlist is intentional. A Codex upgrade must be checked
against the generated app-server schema and the offline conformance suite before
its version is added. DCC does not infer compatibility from a nearby version.

## Session-only projection

Eligible session, project, and global bindings are resolved by the DCC backend.
Their stdio or Streamable HTTP definitions are sent in:

```text
thread/start.params.config.mcp_servers
```

The projection travels over the app-server stdin JSON-RPC channel. DCC does not
pass MCP credentials in command-line arguments, persist them in a temporary
Codex home, or return them to the renderer. The one-shot serialized request is
redacted from debug output and its allocation is zeroized after writing.

Every projected server receives a random per-session `dcc-` wire name. This
prevents DCC definitions from overwriting or relying on a user-owned native
entry. Configuration is bounded and validated before it reaches Codex:

- at most 32 servers;
- at most 128 stdio arguments and 64 secret fields per server;
- no NULs in commands, arguments, paths, or environment values;
- valid environment names;
- valid HTTP header names and values that cannot replace transport-controlled
  headers;
- only `http` and `https` URLs without embedded credentials or fragments;
- no duplicate DCC-owned logical server names.

The app-server process is killed when its DCC session is dropped, and a failed
handshake explicitly starts process termination. Since the projection exists
only in the thread configuration, cleanup never deletes a user-created Codex
entry.

## Remaining verification

This slice establishes safe injection and explicit version gating. It does not
yet claim full Codex conformance. The remaining bridge work is:

1. map app-server MCP startup and inventory status to DCC runtime status;
2. normalize MCP tool lifecycle events;
3. route tool approvals through the DCC permission boundary;
4. run both offline fixture transports through the shared conformance harness;
5. verify direct and configured Codex homes during final end-to-end validation.

Until those gates pass, the public capability remains `nativeConfig`; the
backend-only projection version is internal wiring evidence, not a general
compatibility claim.
