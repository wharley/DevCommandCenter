# MCP definition trust model

This policy defines how DCC records user approval for an external MCP
definition. Trust is local DCC state. It is not inferred from Git history,
provider configuration, a catalog entry, or server annotations.

## Safety boundary

An imported definition is prepared as read-only, disabled, and untrusted. The
activation service only records trust and enables the definition; it does not
start a process, contact a URL, attach a provider, or resolve a secret.

Activation receives the fingerprint shown in the confirmation preview. DCC
recomputes the definition fingerprint after loading it from persistence and
rejects the activation if the two differ. This binds approval to the exact
definition the user reviewed and closes the stale-preview/TOCTOU gap.

Every execution path must require both:

- the definition is enabled; and
- `trust.requires_confirmation()` is false.

## Fingerprint format

`McpDefinition::computed_trust_fingerprint` produces a lowercase SHA-256 digest
using a domain-separated, versioned binary encoding:

```text
dcc-mcp-trust-fingerprint-v1
```

Strings are encoded as UTF-8 with an unsigned 64-bit big-endian byte length.
Lists include their count. Optional fields include an explicit presence tag.
This avoids dependence on JSON serialization, map order, or delimiter
escaping.

The digest includes:

- definition ID;
- transport kind;
- the exact executable, ordered arguments, and optional working directory for
  `stdio`;
- the exact URL for HTTP;
- ownership kind and imported source kind, locator, and definition key;
- secret target kind and name;
- opaque credential reference ID.

HTTP header names are lowercased because their identity is case-insensitive.
Secret bindings are sorted before hashing because their declaration order does
not change behavior. Environment names remain case-sensitive.

The digest excludes display name, enabled state, timestamps, bindings/scopes,
provider exclusions, and every secret value. Changing a selected opaque
credential reference still changes the fingerprint; the credential bytes
never enter it.

## Invalidation

Synchronizing a changed definition updates only `currentFingerprint`. It
deliberately preserves the prior trust decision and its old fingerprint.
Consequently `requires_confirmation()` becomes true without erasing the audit
fact that an earlier version was approved.

Domain validation rejects a definition whose stored current fingerprint does
not match its security-relevant fields. SQLite persistence calls this
validation, preventing callers from accidentally saving an edited definition
with a stale current fingerprint.

## Ownership and lifecycle

Imported definitions remain `ImportedReadOnly`; activation does not transfer
ownership. Disabling or removing DCC state must not modify the imported source
file. Credential deletion remains a separate explicit operation.

Executable resolution and the human-facing activation preview belong to the
probe/UI slices. Adding a resolved executable later must extend the versioned
trust input before command execution is enabled; it must not silently reuse a
fingerprint that covered only the unresolved definition.
