---
name: dcc-orchestration
description: Coordinate native subagents for complex coding tasks in Claude Code, Codex, Gemini CLI, Cursor, or Grok Build. Use for independent exploration, review, tests, or bounded implementation work; keep small sequential tasks and tightly coupled edits in the primary agent.
---

# Coordinate native subagents

Keep the primary agent responsible for requirements, decisions, integration, and the final answer. Use the current provider's native subagents when available; this skill does not create a DCC delegation or switch providers.

## Decide whether to delegate

- Delegate only when it materially improves speed, quality, or context isolation.
- Prefer delegation for independent, read-heavy work such as codebase exploration, documentation research, review, test execution, log analysis, and triage.
- Keep simple, sequential, or tightly coupled work in the primary thread.
- Avoid parallel edits to the same files or subsystem. Partition write tasks by clear ownership boundaries.

## Use the current provider's capabilities

Check the tools and agent roles actually exposed in this session. A provider name or an installed skill alone does not guarantee subagent support in the current runtime.

- **Claude Code:** use the native Agent tool when exposed. Choose an available read-only role for exploration and an edit-capable role for implementation. Use only model aliases accepted by that tool.
- **Codex:** use native spawn_agent and its available follow-up, wait, and close tools. Use explorer or worker roles only when the runtime offers them. Follow the tool's rules for context inheritance and model overrides.
- **Gemini CLI:** delegate through the subagent tools exposed by the active configuration. Respect each agent's tool scope; an investigation agent is not automatically allowed to implement changes.
- **Cursor:** use native subagent delegation when available in the active IDE or CLI session. Do not assume that an IDE capability is present in every CLI mode or model.
- **Grok Build:** use native subagents when enabled. Available explore and plan roles are read-only; use an appropriate edit-capable role for implementation.

If the required capability is missing, perform that work in the primary agent. Do not invent tool names, enable experimental settings, change permissions, or launch a different provider's CLI to simulate delegation.

## Plan the work

- Keep the primary agent as orchestrator and integrator.
- Keep the person's selected model for the primary agent and honor explicit model requests for subtasks.
- For bounded supporting work, prefer a lower-cost native model capable of meeting the assignment's requirements when the runtime supports model selection. Keep complex decisions and final review with the primary agent. If suitable model choices or their relative cost are unknown, inherit the configured defaults. Do not hard-code model versions or map another provider's model names onto this runtime.
- Consider the total cost of delegation, including context transfer, coordination, review, and likely rework. Keep the task in the primary agent when that overhead outweighs the benefit; creating a subagent alone does not guarantee savings.
- Respect the configured concurrency limit. Start only the agents needed for independent work.
- Run dependent tasks after their prerequisites have been reviewed. Read-only roles must report findings rather than attempt edits.

## Preserve the DCC execution environment

- Keep the active checkout, branch, and Local or Worktree choice. This skill does not authorize creating worktrees, changing branches, or applying changes from another checkout.
- In Local mode, account for existing uncommitted changes and other active work. In Worktree mode, keep assignments scoped to the selected worktree.
- Give parallel writers disjoint file ownership. Serialize overlapping changes, dependency installs, migrations, broad formatting, and other operations that affect the whole checkout.
- Do not undo another agent's or the person's changes. If a task needs files outside its assignment, have it report the dependency so the primary agent can revise the plan.

## Write bounded assignments

For every delegated task, state:

1. The concrete objective and boundaries.
2. Whether the task is read-only or may edit files.
3. The files or subsystem the agent owns when edits are allowed.
4. The evidence, validation, or output the agent must return.
5. Relevant constraints from the user and repository instructions.
6. The active checkout and any prerequisites or existing changes to preserve.

Do not delegate vague prompts such as "investigate everything". Do not ask a subagent to redo work already assigned elsewhere.

## Coordinate execution

- Spawn independent tasks in parallel when safe.
- Continue useful primary-thread work while subagents run.
- Send follow-up instructions only when new information materially changes an assignment.
- Wait for every required result before concluding.
- If delegation is unavailable or a subagent fails, continue locally when possible instead of retrying indefinitely.
- When the task is stopped or redirected, stop obsolete child work through the available native controls. Report any child that cannot be stopped; do not leave it silently editing.

## Integrate the result

- Treat subagent output as evidence, not as automatically correct.
- Distinguish a finished agent turn from an accepted result. Check the assignment's completion criteria before starting dependent work or claiming success.
- Reconcile contradictions and inspect material claims before applying them.
- Review the combined diff for overlap, unintended changes, and consistency.
- Run proportionate validation after integration.
- Return one consolidated answer that identifies delegated work only when it helps the user understand the result.
