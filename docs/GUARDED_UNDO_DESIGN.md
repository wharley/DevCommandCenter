# Guarded Undo: Capture v2 and Restoration Contract

Status: Phase 0 implemented and approved (`f1cded2`); Phase 1 in progress

Milestone: M4

Last updated: 2026-08-27

## Decision

Guarded Undo restores the raw content that existed immediately before one
completed agent turn, but only when DCC can prove that the affected files and
repository identity still match the captured result of that turn.

**`captureVersion = 1` is a NO-GO for Undo.** M3 v1 contains immutable review
evidence, Git trees in an isolated temporary object quarantine, rendered
diffs, and manifests. Those records are useful for review, but they are not
restoration artifacts. They MUST NOT be converted, inferred, or used as an
Undo source. The quarantine remains capture-only and may be deleted at any
time.

M4 starts with a new `captureVersion = 2` contract. Capture v2 MUST create a
separate, versioned restoration set containing verified raw-byte preimages and
result fingerprints. If that set is absent, partial, expired, corrupt, or
ineligible, DCC MUST fail closed without changing the workspace.

The safety promise is deliberately limited:

- DCC provides an in-process mutation coordinator, repeated identity and
  fingerprint checks, retained displaced files, and a recovery journal.
- DCC does **not** claim transactionality across several files, immunity from
  another process changing the workspace, or filesystem-wide atomicity.
- A successful operation is verified. A DCC interruption is recoverable from
  the exact files displaced by each per-file exchange, provided those recovery
  artifacts have not themselves been changed or lost. Unexpected external
  mutation enters manual recovery instead of being overwritten.

Normative terms `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are used as described
by RFC 2119.

## Trust boundary

### M3 evidence

The existing `dcc_turn_change_sets` record answers review questions:

- what changed during the turn;
- which baseline and result Git trees were observed;
- which files and validations were reported;
- whether the historical preview is complete, partial, or unavailable.

M3 evidence is immutable after terminal capture and remains readable across
future capture versions. It MUST remain outside the session transcript and
session full-text search.

### M4 restoration artifacts

M4 artifacts answer a different question: which exact bytes may be restored
safely? They live in their own tables and private artifact directory. They
MUST NOT reference an M3 quarantine object as their content source. An M4 set
MAY reference the M3 `snapshot_id` for attribution and UI navigation, but the
raw preimage, its digest, and all eligibility evidence MUST be independently
captured and verified.

Neither M3 nor M4 proves authorship at the operating-system level. The turn
boundary is an observed DCC interval. Capture v2 is eligible only under the
strict conditions below.

Hashes, file identities, and restrictive permissions detect accidental
corruption and integrity drift; they do not defend against a same-user
adversary who can modify both the SQLite database and the private artifact
directory. DCC MUST NOT market Guarded Undo as protection against a malicious
agent or other same-user attacker. The contract is fail-closed integrity
evidence for the local DCC process, not an authenticated security boundary.

## Minimum eligible scope (v1)

The first Guarded Undo release is content-only and intentionally narrow. A
restoration set is `eligible` only when all of these statements are true:

1. The turn has exactly one registered workspace root and that root resolves
   to exactly one Git worktree.
2. `HEAD` exists and the exact object ID and symbolic ref observed at baseline
   equal those observed at result. This is an edge comparison, not a claim that
   no intermediate Git operation occurred.
3. The index fingerprint is identical at baseline and result. A turn that
   leaves the index changed is ineligible. Intermediate index operations that
   return to the observed baseline bytes may not be detected and are not undone.
4. Every attributable path is an existing, tracked, regular file whose Git
   status is exactly content modification (`M`). Additions, deletions,
   renames, copies, type changes, conflicts, unmerged entries, submodules,
   symlinks, reparse points, sparse placeholders, and untracked files are
   ineligible. Ignored paths are outside this contract.
5. Every target has stable raw bytes while its baseline and result fingerprints
   are collected. The baseline raw bytes are persisted as the preimage.
6. File mode, executable bit, ownership-sensitive metadata, and path identity
   observed at baseline equal those observed at result. M4 v1 changes content
   only and makes no claim about intermediate metadata operations.
7. No target uses a Git clean/process filter, working-tree encoding, or other
   attribute conversion that makes the relationship between raw worktree
   bytes and Git evidence ambiguous. Git LFS targets are therefore ineligible.
8. Every target and the complete set fit the limits in this document. Partial
   restoration sets are never eligible.
9. Capture v2 successfully scans every tracked path and stages a raw preimage
   for every tracked regular worktree file at baseline. Unsupported path kinds
   are recorded without following or reading them. Capture MUST NOT predict
   which files the provider will edit. Repositories whose complete regular-file
   baseline exceeds the staging bounds are ineligible in v1.
10. No non-ignored untracked path is present at either the baseline or result
    edge. The full set is ineligible if one is observed; DCC records only path
    evidence for classification and MUST NOT read untracked content. Ignored
    paths remain outside this contract.
11. Every candidate and target regular file has a link count of exactly one.
    Hard-linked files are ineligible because exchanging one directory entry
    cannot truthfully restore the state or aliases of the shared inode.

Pre-existing modifications are permitted: the preimage is the exact raw file
content at the turn boundary, not necessarily the content at `HEAD`. A later
change to an unrelated worktree path does not by itself block Undo and that
path is not an M4 target. This is not a promise that an external writer cannot
race with DCC. A later change to `HEAD`, the ref, the index, the physical root
identity, or any target file blocks automatic Undo when detected.

A commit, push, reset, checkout, index mutation, or ref movement that occurs
between the two observations and returns `HEAD`, ref, and index to the same
captured edge values may be undetectable. Guarded Undo restores eligible target
file content only; it never reverses those Git operations or their external
effects.

## Identity model

Capture, prepare, and execute MUST distinguish these identities instead of
collapsing them into “the repository”:

| Identity | Captured value | Execute requirement |
| --- | --- | --- |
| Workspace/root | Internal `workspace_id` plus an OS-derived physical `root_id`: Unix directory `(st_dev, st_ino)` held by handle; Windows `(volume serial, file ID)` held by handle | Same registered workspace and same physical directory object; aliases resolve to the same identity and lock |
| Git worktree | Stable Git common-dir/worktree-dir identity derived by the Git adapter | Same worktree; no path-only trust |
| `HEAD` | Exact object ID | Equal to captured baseline/result object ID |
| Checkout ref | `symbolic` plus full ref name, or `detached` | M4 v1 requires the same symbolic ref; detached is ineligible |
| Index | SHA-256 over bounded raw index bytes plus relevant stat identity | Equal at baseline, result, prepare, and execute |
| Target worktree file | Normalized repository-relative byte path, size, SHA-256, regular-file identity including link count, and supported metadata fingerprint | Current raw result fingerprint and supported metadata MUST match; link count is exactly one |
| Preimage | Raw bytes, size, SHA-256, and artifact SHA-256 | Artifact digest and length MUST verify before use |

Paths stored for restoration are repository-relative. Absolute repository
paths MUST NOT be written to the restoration tables, logs, analytics, or UI
diagnostics. The implementation MUST reject absolute paths, `..`, NUL, paths
outside the root after resolution, symlink traversal, and platform aliases
that resolve outside the captured target.

`root_id` is not a normalized path or a hash of one. On Unix it MUST be derived
from an opened, no-follow root directory handle and its device/inode identity.
On Windows it MUST be derived from an opened root directory handle and its
volume serial/file ID after rejecting unsupported reparse behavior. The
coordinator maps every workspace path to this physical identity, so
symlink, case, mount, junction, and spelling aliases that reach the same
directory share one lock. If the adapter cannot establish a stable physical
identity for the filesystem, Guarded Undo returns `adapter_unsupported`.

The index fingerprint is evidence, not a lock. Git may replace the index file;
the adapter MUST resolve and read the active index on every validation. An
unreadable or oversized index blocks capture or execution.

## Capture v2 contract

Capture v2 extends the turn lifecycle with a restoration-specific collector.
It MUST NOT weaken or mutate the M3 record.

### Baseline

Before the provider can mutate the workspace, the collector MUST:

1. acquire a shared capture guard from the workspace mutation coordinator;
2. resolve workspace, root, Git worktree, `HEAD`, symbolic ref, and index;
3. enumerate every tracked worktree path and all non-ignored untracked paths
   without invoking external diff or text-conversion drivers; any untracked
   path makes the v1 set ineligible without reading its content;
4. classify unsupported path kinds/attributes without following them, and
   enforce the complete regular-file baseline staging bounds;
5. read every supported, single-link tracked regular file through the platform
   adapter, compute its raw SHA-256, and persist its raw preimage to a temporary
   private artifact;
6. re-read file identity and digest to detect a capture race;
7. fsync the artifact as supported, then atomically publish the artifact within
   the artifact filesystem; and
8. persist the collecting restoration-set metadata.

If step 3 observes an untracked path, the collector records the baseline-edge
classification and skips raw-preimage staging. It MUST still enumerate the
result edge before finalizing the set as `ineligible`, but it MUST NOT read
untracked content at either edge.

If step 4 observes a tracked hardlink, it records
`hardlink_unsupported`, skips raw-preimage staging, and follows the same
edge-completion behavior. It MUST NOT treat one hardlink name as an independent
regular-file preimage.

Baseline collection MUST finish before the turn begins. If it cannot, the turn
may continue, but Guarded Undo for that turn is unavailable with an explicit
reason code.

### Result

At the terminal turn boundary, the collector MUST:

1. reacquire the capture guard and resolve all identities again;
2. verify the observed `HEAD`, symbolic ref, index, and physical root identity
   equal their baseline-edge values;
3. enumerate non-ignored untracked paths again and make the complete set
   ineligible if any are present at this edge, without reading their content;
4. calculate the attributable baseline-to-result path set;
5. require every changed path to satisfy the minimum eligible scope and have a
   verified raw baseline preimage;
6. read and double-check each raw result file, recording its SHA-256, length,
   and supported metadata fingerprint;
7. verify every referenced preimage artifact by length and digest;
8. commit the complete set as `eligible` in one database transaction; and
9. delete the staged preimages for unchanged files.

If an untracked path was observed at either edge, result capture finalizes the
whole set as `ineligible` before artifact eligibility checks. No later absence,
path deletion, or otherwise eligible tracked-file subset changes that outcome.

Capturing the complete bounded baseline is intentionally conservative. A
watcher cannot reliably observe bytes before an external provider overwrites
them, and result-time Git objects are not raw worktree preimages. Future
copy-on-write or write-interception adapters require their own reviewed
contract; v1 MUST NOT substitute either assumption.

If any target is unsupported or any limit is exceeded, the whole set becomes
`ineligible`; it MUST NOT contain a usable subset. I/O failures or invariant
violations produce `failed`. A crash leaves `collecting`, which startup cleanup
converts to `failed` and removes after quarantining incomplete artifacts.

### Persistence model

Names are illustrative but normative fields and relationships are required.

#### `dcc_turn_restore_sets`

| Field | Meaning |
| --- | --- |
| `restore_set_id` | Random UUID primary key |
| `snapshot_id` | M3 attribution reference; never an artifact source |
| `session_id`, `turn_id`, `workspace_id`, `root_id` | Owning DCC identities; `root_id` is the OS-derived physical identity, not a path |
| `capture_version` | Exactly `2` for this contract |
| `state` | Restoration-set state below |
| `reason_code` | Stable machine-readable reason or `NULL` |
| `git_identity_json` | Worktree identity, `HEAD`, ref kind/name, index digest |
| `artifact_bytes`, `file_count` | Retention accounting |
| `manifest_digest` | SHA-256 over canonical versioned file records |
| `created_at`, `completed_at`, `expires_at` | Lifecycle timestamps |

#### `dcc_turn_restore_files`

| Field | Meaning |
| --- | --- |
| `restore_set_id`, `ordinal` | Parent and stable ordering |
| `path_bytes` | Reversible repository-relative path representation; never lossy UTF-8 |
| `status` | Exactly `M` in v1 |
| `pre_size`, `pre_sha256`, `pre_artifact_key` | Raw preimage identity and storage locator |
| `result_size`, `result_sha256` | Required current-content fingerprint |
| `metadata_fingerprint_json` | Adapter-versioned regular-file metadata used for eligibility |

Path representation MUST round-trip non-UTF-8 paths on supported platforms.
Bindings may expose a display-safe escaped form, but mutation APIs use the
opaque backend record, not a UI-supplied path.

#### `dcc_undo_operations` and `dcc_undo_operation_files`

The durable journal records `operation_id`, `restore_set_id`, state, preview
token digest, identity revalidation, timestamps, per-file exchange locators,
expected and displaced identities/digests, per-file application state,
verification outcome, and recovery details. Journal rows and their displaced
files MUST survive until the operation is in a verified terminal state.

### Restoration-set states

| State | Meaning | May prepare? |
| --- | --- | --- |
| `collecting` | Baseline/result capture has not reached a terminal decision | No |
| `eligible` | Complete capture v2 set passed all eligibility checks | Yes |
| `ineligible` | Capture completed but the turn is outside the supported scope | No |
| `failed` | Capture or integrity verification failed | No |
| `expired` | Retention removed the restoration artifacts | No |
| `consumed` | A verified Undo completed | No |

`eligible` MUST mean all artifacts are present and verified. Expiry or missing
content is a state transition, not a best-effort fallback.

### Stable reason codes

Reason codes are API values and MUST remain backward compatible. New codes may
be added; existing meanings may not be reused.

Phase 1A exports no operational filesystem adapter on any platform. An adapter
that lacks a reviewed filesystem-capability and extended-metadata contract MUST
fail closed with `adapter_unsupported`; it MUST NOT create, chmod, inspect, or
claim a private artifact store.

| Class | Codes |
| --- | --- |
| Version/scope | `capture_v1_evidence_only`, `capture_v2_missing`, `unknown_capture_version`, `multiple_roots`, `detached_head`, `bare_repository`, `schema_unsupported`, `invalid_persisted_record` |
| Git identity | `head_changed`, `ref_changed`, `index_changed`, `index_unreadable`, `repository_identity_changed` |
| File kind/status | `unsupported_status`, `unmerged_path`, `untracked_path`, `symlink_or_reparse_point`, `hardlink_unsupported`, `submodule`, `non_regular_file`, `metadata_changed` |
| Git conversion | `git_filter_present`, `working_tree_encoding_present`, `sparse_or_skip_worktree`, `git_attributes_changed`, `assume_unchanged` |
| Bounds | `too_many_baseline_files`, `baseline_too_large`, `too_many_files`, `file_too_large`, `set_too_large`, `index_too_large`, `capture_timeout`, `retention_expired` |
| Integrity/I/O | `capture_race`, `capture_interrupted`, `artifact_missing`, `artifact_corrupt`, `permission_denied`, `io_error`, `workspace_missing`, `artifact_store_unsafe`, `filesystem_unsupported`, `extended_metadata_unsupported`, `insufficient_disk_space` |
| Concurrency | `concurrent_workspace_mutation`, `mutation_in_progress`, `app_instance_conflict`, `path_alias_collision` |
| Baseline manifest | `tracked_manifest_changed` |
| Prepare/execute | `no_target_changes`, `target_missing`, `target_result_mismatch`, `preview_expired`, `preview_consumed`, `preview_context_changed`, `adapter_unsupported` |
| Recovery | `operation_interrupted`, `displaced_target_mismatch`, `displaced_file_missing`, `displaced_file_corrupt`, `recovery_target_changed`, `exchange_rollback_failed`, `manual_recovery_required` |

Raw Git stderr, OS error strings, absolute paths, and file content MUST NOT be
reason codes. They MAY appear behind a local details affordance after redaction.

## Bounds, retention, and privacy

Initial constants MUST be centralized, covered by tests, and changed only with
a migration/retention review:

- maximum 256 target files per restoration set;
- maximum 8 MiB raw preimage per file;
- maximum 32 MiB raw preimages per set;
- maximum 20,000 tracked regular files and 256 MiB of temporary raw preimages
  during complete-baseline staging;
- maximum 64 MiB raw index input;
- maximum 10 seconds for baseline capture and 10 seconds for result capture;
- default artifact retention of 7 days and at most 20 eligible sets per
  workspace; and
- global restoration-artifact budget of 500 MiB.

The oldest eligible sets are expired first when a count or byte limit is
reached. Retention MUST NOT delete artifacts or displaced files referenced by
a nonterminal Undo operation. Workspace/session deletion MUST either delete
terminal artifacts or retain them only until an active recovery is resolved.
Retention and privacy purge are single-instance operations: they MUST run only
while the app-data lifetime lock is held. If ownership cannot be established,
the operation fails closed and leaves all artifacts untouched.

Raw preimages are sensitive local repository content. They MUST:

- remain outside transcripts, FTS, diagnostics bundles, crash reports, and
  telemetry;
- be stored under a DCC-private directory with the strictest practical
  per-user permissions (`0700` directories and `0600` files on Unix);
- use random, non-path-derived artifact names;
- never be logged, rendered without an explicit local preview, or uploaded;
- be removable from a privacy/storage control that reports blocked active
  recoveries; and
- be documented as plaintext-at-rest unless a separately reviewed encryption
  design is implemented. This design does not claim encryption at rest.

Telemetry, if enabled, may report only versioned state/reason buckets, counts,
bounded sizes, and durations. It MUST exclude paths, refs, hashes, workspace
identifiers, content, and preview tokens.

## Prepare and execute APIs

Undo is a two-step command. There is no single-call mutation endpoint.

### `prepare_guarded_undo`

Input:

```text
{ snapshotId }
```

Output is one of:

```text
Ready {
  snapshotId,
  previewToken,
  expiresAt,
  fileCount,
  totalBytes,
  files: [{ displayPath, size, binary, preview }],
  unrelatedPathsAreNotTargets: true
}

Blocked { snapshotId, reasonCode, detailsAvailable }
Unavailable { snapshotId, reasonCode }
```

Prepare MUST be read-only. It resolves the eligible capture v2 set, verifies
artifact integrity, acquires a shared workspace lease, and checks current
root/`HEAD`/ref/index and every target result fingerprint. It generates the
inverse preview from the verified current raw bytes and the M4 preimage, not
from M3 rendered diffs. Binary previews show metadata only.

The preview token MUST be an opaque, cryptographically random, single-use
capability with at most a two-minute lifetime. Server-side token state binds it
to the process instance, workspace, restore set, manifest digest, coordinator
generation, and complete prepared identity. Only a token digest may be
persisted. A second successful prepare invalidates the prior token for that
workspace.

### `execute_guarded_undo`

Input:

```text
{ previewToken, confirmed: true }
```

The backend ignores UI-supplied paths, hashes, or replacement bytes. Execute
MUST:

1. consume the token and acquire the exclusive workspace mutation lease;
2. revalidate everything checked by prepare while holding that lease;
3. stage and verify a same-directory preimage exchange file for every target;
4. durably write the `prepared` journal, including every exchange-file locator
   and expected result identity, before the first workspace mutation;
5. transition to `applying` and exchange each target through the platform
   adapter, leaving the content actually displaced from the target at the
   recorded exchange-file locator;
6. verify and journal both the applied preimage and the displaced file before
   continuing to the next target;
7. verify the complete target set and repository identity; and
8. mark the operation `completed` and the restoration set `consumed`.

Any mismatch before the first exchange returns `blocked` with no workspace
mutation. A failure after the first exchange enters recovery; it MUST NOT be
reported as a normal failure or success.

Execute output states are `completed`, `blocked`, `rolled_back`, or
`recovery_required`. `rolled_back` means a post-mutation failure occurred but
all affected targets were exchanged back and verified against the exact files
displaced by the operation. Repeated calls with a consumed token return
`preview_consumed`; they never run the mutation twice.

## Workspace mutation coordinator

DCC MUST provide one coordinator keyed exclusively by the OS-derived physical
`root_id`. `workspace_id` supplies application context but is not part of the
lock key. Turn baseline/result capture, Guarded Undo, editor writes, Git
index/checkout mutations, delivery operations, and workspace removal MUST
participate.

The lock key MUST use the OS-derived physical `root_id`, not
`workspace_id`, a canonicalized string path, or a Git ref. Multiple workspace
records and path aliases that resolve to the same physical directory MUST
contend on the same lock. The coordinator MUST retain or revalidate the root
directory handle for the lease lifetime; physical identity drift invalidates
the operation.

- Read-only preparation and capture use shared leases where safe.
- Execute and other workspace mutations use an exclusive lease.
- Every lease observes a monotonically increasing generation. A prepared token
  is invalid after the generation changes.
- Phase 1 MUST maintain a minimum active-interval registry keyed by
  `PhysicalRootId`, recording the owning `(session_id, turn_id)` and the
  baseline generation. A second known DCC turn or capture interval on the
  same physical root overlaps the first; the affected restoration set MUST
  finalize as `ineligible` with `concurrent_workspace_mutation`.
- Every known DCC mutator (editor save, Git/index/checkout action, delivery
  mutation, or workspace removal) MUST acquire the exclusive lease and dirty
  the root generation. A generation change between a turn's baseline and
  result edges makes that capture `ineligible` with
  `concurrent_workspace_mutation` (or
  `mutation_in_progress` when no turn interval owns the root).
- Lock ordering for multi-workspace operations is stable by opaque root ID.
  M4 v1 itself accepts only one root.
- The coordinator is cancellation-safe and releases leases on unwind.

The process MUST acquire a single-instance lifetime lock in the DCC app-data
directory before startup recovery, retention, artifact purge, or interval
registry initialization. Phase 1 selects this lock as the minimum ownership
mechanism. If it cannot be acquired, startup MUST fail closed and MUST NOT
run cleanup, retention, recovery, or capture; no second process may delete or
rewrite artifacts owned by the lock holder. The lock is held for the complete
process lifetime and released only during orderly shutdown.

This coordinator covers only mutations made by the current DCC process.
Terminals, editors, Git hooks, filesystem watchers, other DCC processes, and
external programs can still race. Consequently, execute repeats on-disk checks
immediately before each exchange and relies on the exact displaced files and
journal for recovery. The UI MUST NOT describe the operation as globally
atomic.

## Durable journal, displaced files, and recovery

Operation states are:

| State | Meaning |
| --- | --- |
| `preparing` | Preimage exchange-file staging; no mutation is permitted yet |
| `prepared` | Every exchange file is durable, verified, and journaled |
| `applying` | At least one exchange may occur |
| `verifying` | All planned exchanges were attempted; final verification pending |
| `completed` | Every target matches its preimage and identity checks pass |
| `rolling_back` | DCC is exchanging validated displaced files back after a partial apply |
| `rolled_back` | Every affected target is verified against the exact file displaced by its exchange |
| `recovery_required` | Automated progress would risk overwriting unexpected content |

Each file mutation MUST use an OS/filesystem primitive with documented
exchange or replace-with-backup semantics. In one per-file operation, the
preimage becomes the target and the content actually displaced from the target
remains at a predeclared, same-directory recovery locator. A copy read before
the exchange MAY be used for diagnostics, but it is not authoritative recovery
content and MUST NOT be trusted as proof of what the primitive displaced.

Immediately after the exchange, DCC MUST validate the displaced file's
physical identity, raw length, digest, and supported metadata against the
expected result that was revalidated before the operation. DCC MUST retain the
displaced file until the operation is terminal and retention permits cleanup.
Only the validated displaced file is the per-file rollback source.

If the displaced file differs from the expected result, DCC has detected a
cross-process race. It MUST stop before the next target and attempt an exchange
rollback only when (a) the target still exactly matches the preimage installed
by this operation and (b) the displaced file still exactly matches the
unexpected identity just observed. That rollback exchanges the files again;
it does not copy over either one. If either condition fails, or the exchange
rollback cannot be verified, DCC enters `recovery_required`, retains both
files, and MUST NOT perform another automatic overwrite.

After a normal exchange, DCC MUST verify the target preimage hash, validate the
displaced result, and persist per-file progress. These displaced artifacts are
raw local repository content and follow the same privacy and retention rules
as other recovery artifacts; they are independent of M3/M4 historical
previews.

On startup, the recovery manager examines every nonterminal operation:

- if no target changed, all still match the expected result, and every staged
  exchange file still matches its preimage, mark it rolled back;
- if every target matches its preimage and every displaced file validates as
  the expected result actually displaced by its exchange, finish verification
  and mark completed;
- for a clean mixed state, rollback only files that still exactly match the
  preimage written by that operation, using their exact validated displaced
  files and the same exchange primitive; or
- if any file is unexpected or an artifact is missing/corrupt, enter
  `recovery_required`, block new mutations for that workspace, preserve all
  artifacts, and show a manual recovery workflow.

Automated recovery MUST never copy over a file whose bytes or supported
metadata differ from the journal's expected state. It uses exchange only when
both sides match their journaled identities. Manual recovery exports clearly
labeled displaced/preimage copies to a user-selected location; it does not
silently apply them.

## Platform file adapter

Workspace mutation is implemented behind an audited platform adapter. The
adapter MUST provide:

- no-follow, regular-file inspection and repository containment checks;
- bounded raw reads with identity-before/read/identity-after race detection;
- private artifact and same-directory exchange-file creation;
- a per-file exchange or replace-with-backup primitive that retains the file
  actually displaced from the target at the predeclared recovery locator;
- application and validation of the expected supported metadata on the
  preimage exchange file;
- file and parent-directory durability calls where the platform supports them;
- post-exchange target and displaced-file identity/digest verification; and
- capability detection that returns `adapter_unsupported` instead of falling
  back to an unsafe write.

On Unix, every path-component resolution and mutation MUST be relative to the
held physical root or parent directory handles and MUST use no-follow semantics
(`openat2` constraints where available, otherwise an audited component-by-
component `openat`/`fstatat` strategy). A string-based canonicalization check is
not a substitute. If any required no-follow or handle-relative guarantee is
unavailable, the adapter returns `adapter_unsupported`.

On Windows, the adapter MUST anchor resolution to the physical root handle,
open and inspect every component without transparently traversing reparse
points, compare volume serial/file IDs, and reject unsupported junction,
symlink, mount-point, or other reparse behavior. The exchange MUST use a
reviewed replace-with-backup primitive that retains the actually displaced
file. If equivalent handle anchoring, reparse rejection, or displaced-file
retention is unavailable on the target filesystem, the adapter returns
`adapter_unsupported`.

Read-only flags, ACLs, extended attributes, alternate data streams, executable
bits, and ownership semantics require explicit adapter tests. An adapter that
cannot apply and verify the expected relevant metadata on the installed
preimage MUST mark the target ineligible.

Inspection MUST include link count. A regular file with `nlink > 1` on Unix, or
an equivalent multi-link indication on Windows, returns
`hardlink_unsupported`; DCC MUST NOT exchange any one of its names.

The required exchange/replace-with-backup primitive is atomic only for the one
target/displaced-file transition promised by that platform primitive. It is
not a transaction across files and does not prevent an external process from
writing immediately before or after it. Network filesystems, virtualized
mounts, and filesystems without the required exchange, durability, physical
identity, displaced-file retention, or no-follow semantics MUST be blocked
until their adapter is reviewed.

## UI flow

1. `Undo last turn` is enabled only when an eligible, unconsumed capture v2 set
   is known. Other snapshots show `Undo unavailable` and a concise reason.
2. Activation calls prepare. The UI shows the exact target count, inverse
   previews, binary-file metadata, expiry, and the statement: “Later changes
   outside these files are not Undo targets. Changes to these files, Git HEAD,
   branch, or index block automatic Undo when detected. External programs can
   still race with this operation.”
3. The confirmation button names the action, for example `Restore 3 files`.
   Closing the dialog or token expiry causes no mutation.
4. Execution displays `Verifying`, `Staging recovery exchange files`,
   `Restoring`, and `Verifying result`; it disables conflicting DCC actions for
   the root.
5. `blocked` refreshes the review and offers a manual diff/export route.
   `recovery_required` opens a persistent recovery surface and does not offer
   another Undo.
6. Completion links back to Changes and retains a local audit row without
   retaining artifacts beyond the documented terminal-operation policy.

The UI MUST distinguish unsupported, expired, changed-after-turn, corrupt, and
recovery-required states. It MUST NOT turn a reason-code failure into a generic
toast or imply that external concurrent edits were impossible.

## Phase 1 turn-lifecycle integration

Capture v2 begins after the durable `TurnStarted`/active-turn claim and before
`send_provider_input` in all three production paths: the desktop
`commands::session_commands::send_turn`, the HTTP `send_turn_handler`, and
`SessionCommandState::dispatch_next_queued_turn`. At the terminal edge, DCC
first appends or finds the unique durable terminal event and then immediately
polls the cancellation-safe M4 finalizer in the same persistence future. The
event's canonical outcome selects the finalization mode; event publication,
arbiter commit, binding cleanup, and command return wait for that finalizer.
This ordering prevents a competing terminal request from making restoration
eligibility authoritative while still allowing a retry to finish an active
capture after cancellation or process-local interruption. `quiesce_turn_for_abort`
and `cancel_provider_session` use the same idempotent finalizer. The
`(session_id, turn_id, workspace_id, snapshot_id)` binding is the ownership key,
while the physical root identity is the coordinator key.

Capture work MUST run through `spawn_blocking` with a bounded deadline and
cancellation-safe cleanup. A capture failure is persisted as `failed` or
`ineligible` with a stable reason code and is never allowed to block, suppress,
or duplicate the terminal `TurnCompleted`/`TurnAborted` event. Provider stream
termination without a terminal event marks an in-progress capture as
`failed(operation_interrupted)` while the process is alive. Phase 1 initializes
recovery lazily at the first feature-enabled capture attempt: it acquires the
single-instance lifetime lock and converts leftover `collecting` rows to
`failed(capture_interrupted)` before admitting any new M4 begin. Until that
gate succeeds, it performs neither capture nor global cleanup.

## Phased implementation

### Phase 0 — Schema and fixtures (implemented and approved in `f1cded2`)

- Add migration-tested restoration and journal tables.
- Add canonical manifest serialization, reason-code types, size accounting,
  and fixture compatibility tests.
- Keep capture v1 read-only and explicitly map it to
  `capture_v1_evidence_only`.

### Phase 1 — Capture v2, no Undo button

- Implement platform inspection and private raw-preimage storage behind a
  feature flag.
- Implement the minimum physical-root coordinator and active-interval
  registry before the provider can mutate a turn. Known overlapping DCC
  intervals and DCC mutations dirty the generation and make the affected
  capture ineligible; external processes remain residual races.
- Capture baseline/result identities and classify eligibility.
- Run baseline/result I/O in `spawn_blocking` (with cancellation and bounded
  deadlines) so capture cannot block the async/UI runtime. Capture failures
  MUST persist `failed` or `ineligible` and MUST NOT prevent the terminal
  `TurnCompleted` or `TurnAborted` event from being recorded and published.
- Acquire the app-data single-instance lifetime lock before startup cleanup,
  retention, integrity audit, privacy controls, or capture; failure to acquire
  it is fail-closed and performs no cleanup.
- Keep the Phase 1 implementation behind a feature flag. Windows remains
  explicitly `adapter_unsupported` until the handle-relative,
  reparse-rejecting adapter and its tests are complete.
- Compare M3 evidence with M4 classification in tests without using M3 as an
  artifact source.

M3 remains a review-only surface: `captureVersion = 1` is always NO-GO for
Undo. Phase 1 MUST include a sentinel filter test proving that no capture,
preview, or restoration path can consume M3 trees, rendered diffs, or the M3
quarantine as restoration content.

### Phase 2 — Read-only prepare and preview

- Add only the read-only prepare API, bounded inverse previews, and expiring
  single-use tokens bound to the Phase 1 manifest and coordinator generation.
- Exercise later unrelated edits, target edits, Git/index/ref changes, and
  external-race fixtures.

### Phase 3 — Execute and recovery

- Add exclusive coordination, verified exchange files, durable journaling,
  exchange/replace-with-backup adapters, displaced-file validation,
  post-verification, startup recovery, and recovery UI.
- Ship disabled by default until crash/fault-injection tests pass on every
  enabled platform/filesystem combination.

### Phase 4 — Product rollout

- Enable for the minimum `M`-only scope, add EN/PT-BR copy, accessibility, and
  content-free local metrics.
- Publish the exact limitations and a user-visible artifact purge control.

### Later phases

Additions/deletions, renames, executable-bit changes, symlinks, untracked files,
multi-root turns, detached HEAD, filters/LFS, and three-way reconciliation each
require a separate design extension. None may silently broaden v1 eligibility.

## Acceptance and test matrix

All tests assert both filesystem result and journal/database state. Fault tests
run at every journal boundary and before/after every exchange.

| Area | Required cases |
| --- | --- |
| Version boundary | Capture v1 is always NO-GO; missing/unknown future versions fail closed; M3 preview remains readable |
| Eligible capture | One and many tracked regular single-link `M` files; pre-existing modifications; raw binary and non-UTF-8 bytes; equal observed HEAD/ref/index edges |
| Unsupported capture | A/D/R/C/T/U, symlink, reparse point, hardlink, submodule, conflict, filter/LFS, working-tree encoding, sparse/skip-worktree, detached HEAD, multiple roots |
| Untracked boundary | Pre-existing untracked file unchanged, modified, or deleted during the turn; new result untracked file; every case makes the whole set ineligible without reading content; ignored paths remain out of scope |
| Bounds | Exactly-at and over baseline file/byte staging, target file count, per-file bytes, set bytes, index bytes, and deadline; no partial eligible set |
| Attribution/races | File changes during baseline read, provider startup, result read, and database commit; capture fails closed |
| Prepare | Ready preview from M4 preimage; target changed/deleted/replaced; HEAD/ref/index/physical root changed; an unrelated worktree path is not targeted; corrupt/missing/expired artifact |
| Tokens | Entropy, expiry, single use, second-prepare invalidation, process restart, coordinator-generation change, wrong workspace/snapshot |
| Execute | Revalidation before any write; successful one/many-file restore; preview path tampering ignored; consumed set cannot repeat |
| External races | Change before staging, before exchange, during the platform primitive, between exchanges, and after exchange; validate the actual displaced file, exchange-roll back only when both identities match, otherwise require recovery; tests do not claim exclusion of the residual cross-process race |
| Fault/crash recovery | Failure at every journal transition, short write, fsync error, disk full, permission change, process kill, corrupt/missing displaced artifact; verified exchange rollback or `recovery_required` |
| Metadata/platform | Unix handle-relative component/no-follow enforcement, dev/inode alias lock identity, mode/xattr/ACL capability, macOS metadata, Windows volume/file-ID alias identity, ACL/read-only/reparse behavior, case-folding and Unicode aliases, locked files, unsupported/network filesystem |
| Coordinator | Capture versus editor save, terminal-triggered Git action, delivery mutation, workspace deletion, cancellation, and stable lock ordering |
| Retention/privacy | Seven-day/count/global eviction, active journal exemption, purge, restrictive permissions, no content/path/ref/hash in logs/FTS/telemetry |
| UI/accessibility | Keyboard and screen-reader confirmation, explicit file count/action, blocked reason, token expiry, recovery persistence, EN/PT-BR parity |

Release acceptance requires the frontend suite, infrastructure tests, Tauri
tests, migration tests, adapter tests, and fault-injection suite to pass in CI.
Manual QA MUST include killing the process during a multi-file operation and
editing a target from an external editor during prepare and execute.

## Non-goals

- Git reset, checkout, restore, clean, force-push, hook bypass, or rewriting
  branch history as an Undo implementation.
- Restoring from M3 Git trees, rendered diffs, transcript content, current Git
  objects, or the isolated snapshot quarantine.
- Undoing commits, pushes, pull requests, dependency installs, commands,
  database migrations, generated external state, or provider-side actions.
- Detecting or reversing an intermediate commit, push, reset, checkout, ref
  movement, or index operation that returns all observed edge identities to
  their captured values.
- Silently deleting untracked files or reverting staged/index changes.
- Claiming authorship of every change observed during a turn.
- Cross-process locks, multi-file filesystem transactions, or a guarantee that
  external tools cannot race.
- Automatic conflict resolution or three-way merge in the first release.
- Cloud backup, artifact upload, telemetry of repository content, or encryption
  at rest without a separate reviewed design.

## Contributor checklist

Any Guarded Undo contribution MUST answer:

1. Which identity or fingerprint is checked before the mutation?
2. Which reason code represents every fail-closed path?
3. What durable state exists if the process stops on the next instruction?
4. Which exact displaced file recovers an already exchanged target, and how is
   that displaced identity validated?
5. How are symlinks, reparse points, filters, limits, and non-UTF-8 paths tested?
6. Could the change read or expose repository content outside the local
   restoration store?
7. Does it preserve the rule that capture v1 and M3 quarantine are never Undo
   sources?

Changes to capture schema, eligibility, mutation adapters, journal/recovery,
retention, or reason-code semantics require core-maintainer and security
review.
