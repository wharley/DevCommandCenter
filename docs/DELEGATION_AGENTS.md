# Delegation Agents

Delegation agents let a parent session hand work to a child session without
polluting the parent workspace. The child runs in an isolated delegation
worktree, produces reviewable output, and returns control to the parent before
anything is applied.

## What It Does

- Creates child sessions for review, explanation, or implementation tasks.
- Runs implementation work in an isolated Git worktree.
- Shows delegation progress in the parent session and Activity view.
- Opens delegated changes in the Inspector with the changed-file tree and real
  diffs.
- Routes feedback from a selected diff back to the child session so the agent
  keeps the right context.
- Applies accepted work back into the parent worktree or discards it.
- Cleans up child worktrees after apply, discard, and workspace deletion.

## Typical Flow

Delegating is the same send you already do, pointed at another agent.

1. Start from a parent session in a workspace.
2. Type the task in the composer, exactly as you would for the active agent.
3. Open the menu next to the Send button and pick a target agent. That one click
   starts the delegation. The instruction is the composer text, the model is the
   target's recommended one, and the execution dials are the effort and fast-mode
   settings already selected in the composer.
4. Let the child session run.
5. When implementation output is ready, open it in the Inspector.
6. Review the changed-file tree and diffs.
7. Send feedback from a diff when needed; DCC routes it to the child session.
8. Apply the result to the parent worktree or discard the delegation.

The parent agent can also request a delegation itself; that arrives as an
approval card in the thread rather than as a form.

### Switching providers in one session

DCC persists the shared timeline, but provider runtimes do not expose native
1:1 memory to one another. When the provider is changed in an existing session,
the next direct turn receives one deterministic, bounded re-anchor containing
workspace/Git metadata, available mission and plan context, and recent durable
user/assistant messages. The user's latest prompt remains authoritative;
the packet is context, not a new instruction. It is intentionally limited and
does not include a full transcript, tool noise, or reasoning. Starting a new
thread always means starting fresh.

### Options in the delegate menu

- **Can edit files** — off by default. Turning it on makes this an implementation
  delegation, which runs in an isolated worktree and narrows the target list to
  agents that support edits.
- **Delegate to several** — sends the same task to multiple agents at once, each
  in its own child session, so their answers can be compared. Read-only.

### What DCC decides for you

Mode and context are derived rather than asked:

| Situation | Mode | Context sent |
|---|---|---|
| File edits allowed | Implement | Session summary with spec, plan, and diff |
| Read-only, worktree has changes | Review | Instruction plus the current diff and changed files |
| Read-only, worktree is clean | Explain | Instruction plus workspace metadata |

Each delegation card shows the decisions that were made for that run, so the
choices stay visible without becoming questions.

## Plan Handoff Flow

For planner/executor workflows, keep the planning session as the parent and
delegate the approved plan to an implementation model.

1. Start a parent session with the planner model, for example Claude Fable.
2. Enable Plan mode and ask for the frontend/design plan.
3. Approve the plan. This is the human checkpoint.
4. Click `Delegate implementation`. DCC starts the delegation directly with the
   full plan, implementation criteria, `Implement` mode, full reanchor context,
   and an edit-capable target from the catalog.
5. Review the child implementation diff in the Inspector.
6. Apply the result to the parent worktree or discard the isolated worktree.

This avoids copying the plan by hand and avoids re-asking for decisions that plan
approval already settled, while keeping the human checkpoint between planning and
implementation.

## Inspector Review

Implementation delegations are reviewed from the Inspector. During review, the
Inspector points at the child worktree, not the parent worktree. This makes the
changed-file tree and diffs show exactly what the child agent changed.

When you apply the delegation, DCC freezes tracked, staged, unstaged, and new
untracked regular files into a private pre/post manifest before the first
parent-worktree write. This includes newly created files such as database
migrations. The parent worktree must be clean before applying so DCC does not
overwrite local work. Pre-existing symlink ancestors, hardlinks, submodules,
special files, non-UTF-8 paths, and case-colliding path sets fail closed instead
of being copied with ambiguous semantics. Renames are frozen as an explicit
delete plus add so both names participate in apply and rollback.

Apply is transactionally journaled in SQLite and does not depend on Guarded
Undo or on the macOS DMG capture path. The journal binds the delegation
operation, Git HEAD/ref/index identity, manifest digest, artifact accounting,
owner, and expiring recovery lease. A process-held operation lock prevents an
expired lease from being taken over while its original DCC process is still
alive. Git inspection is non-interactive and time-bounded. File replacement
uses same-directory atomic installation and preserves Unix permission bits;
the Git index is not modified.

If DCC stops while applying, startup classifies every destination path against
the frozen manifest. An all-post destination completes as applied, an all-pre
destination returns to review, and a known partial destination is restored to
its complete preimage. Any external divergence, corrupt artifact, unexpected
temporary file, or changed Git identity is preserved and reported as requiring
manual recovery. DCC will not remove the child worktree while that recovery
authority is active.

The artifact contract covers regular-file contents, absence/presence, and Unix
permission bits. Filesystem-specific ACLs, extended attributes, and directory
metadata are outside this content-apply contract; paths that need unsupported
link or special-file semantics are rejected instead of approximated.

## Rerunning a Delegation

A finished read-only delegation can be replayed on a different agent from its
card. The rerun sends the stored prompt verbatim, so both runs receive the same
task and the same context and their answers stay comparable.

Implementation delegations cannot be rerun: their prompt pins the child worktree
path, and that worktree is removed once the delegation is applied or discarded.

## Cleanup Behavior

After a delegation is applied or discarded, DCC removes the child worktree and
returns the review surface to the parent context. When a workspace is deleted,
DCC also cleans up its journaled delegation worktrees before changing remote
state or removing the primary workspace.

Each implementation worktree has an opaque durable operation ID. The journal
binds that ID to the workspace, parent and child sessions, delegation, source
root, worktree path, branch, baseline commit, and expected branch OID. Apply and
discard resolve this record by ID; `workingDirectoryOverride` remains provider
context and preview data, but is not deletion authority.

Lifecycle updates use compare-and-swap transitions. If DCC stops during
prepare, bind, apply, or removal, startup reconciliation resumes only work
whose mutation intent is already durable. A missing ref is an idempotent
success, while a branch that advanced to another OID is preserved and reported
for recovery. Apply and removal use separate durable expiring leases, so only
one DCC process may own each lifecycle mutation and a later process can take
over after a crash. The apply operation lock also protects long-running
preparation and filesystem I/O from lease-expiry overlap. Terminal delegations
and private apply artifacts are
cleaned immediately when possible and reconciled on startup. Interrupted or
potentially partial apply is never reported as success.

## Notes

- Delegation is provider-aware. Only providers with compatible capabilities are
  offered as delegation targets for the selected permission level.
- The parent session remains the coordination point, while the child session
  owns the delegated context.
- Delegation review is designed to avoid affecting the normal non-delegated
  Git and Inspector flows.
