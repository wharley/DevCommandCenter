# MCP open source review

This record covers the external MCP integrations work reviewed through commit
`4c4fe4a` on July 28, 2026. It is an engineering inventory, not legal advice,
and does not replace a full release-wide software composition analysis.

## Result

The MCP implementation adds no bundled provider CLI, remote MCP service, Figma
client, Garu package, Factory SDK, Grok source, Gemini CLI, or Cursor CLI. DCC
implements provider protocol boundaries in its own Apache-2.0 Rust and
JavaScript source and invokes installed provider runtimes only when the user
selects them.

The direct dependency additions attributable to the MCP work use permissive
licenses compatible with the repository's Apache-2.0 source distribution:

| Component | Version | Use | Declared license |
| --- | --- | --- | --- |
| `keyring` and selected platform stores | `4.1.5` | OS credential-store adapter | MIT OR Apache-2.0 |
| `zeroize` | `1.8.2` | Reduce lifetime of backend-only secret buffers | Apache-2.0 OR MIT |
| `tokio-stream` | `0.1.18` | Repository-owned offline fixture | MIT |
| `tower` | `0.5.3` | Offline fixture tests | MIT |

The Cargo metadata review also confirmed MIT or Apache-2.0 declarations for
`keyring-core`, `apple-native-keyring-store`,
`windows-native-keyring-store`, `zbus-secret-service-keyring-store`,
`secret-service`, `security-framework`, and `zbus` in the selected lockfile.

All DCC Cargo and Node workspace packages now declare `Apache-2.0` explicitly,
matching the root [LICENSE](../LICENSE). The internal packages remain
non-publishable/private; the metadata prevents ambiguous license reporting in
composition tools.

## External and opt-in components

These components are not DCC dependencies and must not be represented as
bundled:

| Component | Relationship to DCC | Review outcome |
| --- | --- | --- |
| Figma remote MCP | User-authenticated external service | No code is installed or redistributed by DCC |
| `@garuhq/mcp@0.17.0` | Exact-version `npx` smoke with dedicated acknowledgement | npm metadata declares MIT; package is absent from DCC manifests and lockfiles |
| Factory Droid SDK `0.6.0` | Public protocol reference only | Upstream SDK is Apache-2.0; DCC vendors no SDK or proprietary Droid CLI |
| Grok Build | Protocol/runtime reference only | Upstream repository is Apache-2.0; DCC vendors no Grok source or binary |
| Gemini CLI | User-installed provider runtime | Not bundled by the MCP bridge |
| Cursor CLI | User-installed provider runtime | Not bundled by the MCP bridge |
| Codex CLI | User-installed provider runtime | Not added to the DCC dependency graph by the MCP bridge |

The Garu smoke pins the package name and version and requires an exact
supply-chain acknowledgement. Registry metadata inspected on July 28, 2026
reported integrity
`sha512-U1A3IXiIIcDGl1myqS/Z9pdcL8kfNeEJLbycyWoDAgCChrqIhwCg3nyWcq29Lt38zsgjPMqW8O0EwBBcd/8VKQ==`.
This value is audit evidence, not a DCC installer or permission to execute the
package without the existing opt-in gate.

## Pre-existing distribution item

The root application already depended on
`@anthropic-ai/claude-agent-sdk@0.2.126` and
`@anthropic-ai/claude-code@2.1.126` before the external MCP roadmap began.
Their installed npm metadata declares `SEE LICENSE IN README.md`, and those
README files refer users to Anthropic terms rather than an SPDX open-source
license.

The MCP work did not introduce these packages, but the Claude bridge uses the
existing sidecar. A release-wide distribution review must therefore confirm
that the way signed DCC artifacts stage and ship those packages complies with
their current terms. This review is a release blocker and must not be marked
complete merely because the new MCP-specific Rust dependencies are permissive.

## Security reporting review

The root [security policy](../SECURITY.md) now explicitly covers:

- untrusted project MCP command activation;
- command, URL, header, environment, and working-directory validation;
- credential leakage and OAuth scope confusion;
- cross-server ownership and approval-policy bypasses;
- provider-owned configuration deletion;
- child-process cleanup; and
- pinned third-party execution.

The repository's GitHub private vulnerability reporting setting was confirmed
disabled through the GitHub API on July 28, 2026. Enabling it is an external
repository administration action and remains a release blocker. Until then,
the public security-contact issue form permits only a request for a private
channel and forbids vulnerability details.

## Required release actions

- Enable GitHub private vulnerability reporting and verify the private report
  form is visible from a non-administrator account.
- Confirm security-alert notifications reach at least one active maintainer.
- Complete and record the Anthropic package distribution review.
- Generate the normal release-wide third-party notices or composition report;
  do not treat this scoped MCP inventory as exhaustive.
- Re-run the dependency review whenever an MCP dependency, external command
  version, or bundled provider runtime changes.
- Keep authenticated and real-service tests ignored in public/fork CI.
