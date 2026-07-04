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

1. Start from a parent session in a workspace.
2. Ask the parent agent to delegate a task, or use the delegation controls in
   the session header.
3. Choose the target provider and mode:
   - `Review` for an independent pass over the work.
   - `Explain` for investigation and analysis.
   - `Implement` for isolated code changes.
4. Let the child session run.
5. When implementation output is ready, open it in the Inspector.
6. Review the changed-file tree and diffs.
7. Send feedback from a diff when needed; DCC routes it to the child session.
8. Apply the result to the parent worktree or discard the delegation.

## Inspector Review

Implementation delegations are reviewed from the Inspector. During review, the
Inspector points at the child worktree, not the parent worktree. This makes the
changed-file tree and diffs show exactly what the child agent changed.

When you apply the delegation, DCC copies tracked, staged, unstaged, and new
untracked files back into the parent worktree. This includes newly created
files such as database migrations. The parent worktree must be clean before
applying so DCC does not overwrite local work.

## Cleanup Behavior

After a delegation is applied or discarded, DCC removes the child worktree and
returns the review surface to the parent context. When a workspace is deleted,
DCC also cleans up delegation worktrees associated with child sessions so stale
worktrees are not left behind.

## Notes

- Delegation is provider-aware. Only providers with compatible capabilities are
  offered as delegation targets for the selected mode.
- The parent session remains the coordination point, while the child session
  owns the delegated context.
- Delegation review is designed to avoid affecting the normal non-delegated
  Git and Inspector flows.
