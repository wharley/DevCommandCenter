# Third-party notices

This file records third-party software that DCC redistributes in its release
artifacts. It is an engineering attribution record and does not replace a
release-wide software composition review.

## ai-memory

- Project: [akitaonrails/ai-memory](https://github.com/akitaonrails/ai-memory)
- Pinned release in DCC artifacts: `v2.2.2`
- License: [MIT](https://github.com/akitaonrails/ai-memory/blob/main/LICENSE)
- Use: optional local sidecar for project memory, scoped retrieval, and
  cross-session handoff
- DCC changes: DCC uses its own adapter, SQLite outbox, retry policy, and UI;
  the upstream binary is not modified or presented as DCC source code
- Packaging: release builds verify the upstream archive checksum before
  placing the sidecar in the application bundle

The source repository, upstream license, and release terms remain authoritative
for ai-memory. Update this notice and the pinned checksum whenever the bundled
version changes.
