---
name: dcc-orchestration
description: Orchestrate Codex subagents for complex multi-part coding work. Use when independent exploration, review, tests, or bounded implementation streams can run in parallel; do not use for small sequential tasks or tightly coupled concurrent edits.
---

# Orchestrate with Codex subagents

Coordinate specialized subagents while keeping the primary agent responsible for requirements, decisions, integration, and the final answer.

## Decide whether to delegate

- Delegate only when it materially improves speed, quality, or context isolation.
- Prefer delegation for independent, read-heavy work such as codebase exploration, documentation research, review, test execution, log analysis, and triage.
- Keep simple, sequential, or tightly coupled work in the primary thread.
- Avoid parallel edits to the same files or subsystem. Partition write tasks by clear ownership boundaries.

## Plan the agent team

- Keep the primary agent as orchestrator and integrator.
- Use the built-in `explorer` role for read-only codebase mapping and evidence gathering.
- Use the built-in `worker` role for a bounded implementation or fix with explicit file ownership.
- Use `gpt-5.6-terra` for fast exploration, review, tests, and other well-scoped supporting work. Reserve `gpt-5.6-sol` for a subtask that truly requires deeper ambiguous reasoning.
- Respect the configured concurrency limit. Start the smallest useful team, normally one to three subagents.

## Write bounded assignments

For every delegated task, state:

1. The concrete objective and boundaries.
2. Whether the task is read-only or may edit files.
3. The files or subsystem the agent owns when edits are allowed.
4. The evidence, validation, or output the agent must return.
5. Relevant constraints from the user and repository instructions.

Do not delegate vague prompts such as "investigate everything". Do not ask a subagent to redo work already assigned elsewhere.

## Coordinate execution

- Spawn independent tasks in parallel when safe.
- Continue useful primary-thread work while subagents run.
- Send follow-up instructions only when new information materially changes an assignment.
- Wait for every required result before concluding.
- If delegation is unavailable or a subagent fails, continue locally when possible instead of retrying indefinitely.

## Integrate the result

- Treat subagent output as evidence, not as automatically correct.
- Reconcile contradictions and inspect material claims before applying them.
- Review the combined diff for overlap, unintended changes, and consistency.
- Run proportionate validation after integration.
- Return one consolidated answer that identifies delegated work only when it helps the user understand the result.
