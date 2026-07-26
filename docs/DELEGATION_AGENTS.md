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

When you apply the delegation, DCC copies tracked, staged, unstaged, and new
untracked files back into the parent worktree. This includes newly created
files such as database migrations. The parent worktree must be clean before
applying so DCC does not overwrite local work.

## Rerunning a Delegation

A finished read-only delegation can be replayed on a different agent from its
card. The rerun sends the stored prompt verbatim, so both runs receive the same
task and the same context and their answers stay comparable.

Implementation delegations cannot be rerun: their prompt pins the child worktree
path, and that worktree is removed once the delegation is applied or discarded.

## Cleanup Behavior

After a delegation is applied or discarded, DCC removes the child worktree and
returns the review surface to the parent context. When a workspace is deleted,
DCC also cleans up delegation worktrees associated with child sessions so stale
worktrees are not left behind.

## Notes

- Delegation is provider-aware. Only providers with compatible capabilities are
  offered as delegation targets for the selected permission level.
- The parent session remains the coordination point, while the child session
  owns the delegated context.
- Delegation review is designed to avoid affecting the normal non-delegated
  Git and Inspector flows.
