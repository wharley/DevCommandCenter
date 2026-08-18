# Codex orchestration in DCC

This guide explains how to use GPT-5.6 Sol as the primary agent and delegate parts of a task to Terra subagents in Dev Command Center (DCC).

## How orchestration works

Sol remains responsible for understanding the request, making decisions, integrating the results, and delivering the final response. When a task can be divided safely, it can start Terra subagents to research, review, test, or implement independent parts in parallel.

Delegation does not happen for every message. Small or sequential requests, and tasks that require tightly coupled changes, normally remain in the primary thread. This avoids unnecessary latency and usage.

## Install the orchestration preset

The preset is installed per project:

1. Open the project in DCC.
2. Open **Skills**.
3. Click **Orchestration preset**.
4. Keep the default values.
5. Confirm that **Codex** is selected under **Target agents**.
6. Leave **Disable model invocation** turned off.
7. Click **Save skill**.

When you save, DCC creates the `dcc-orchestration` skill and automatically compiles it into the active worktree. The button then displays **Orchestration installed**.

Create a new session after installing the preset. A session that was already open may have loaded the previous skill catalog.

## Use orchestration automatically

In the new session, select **GPT-5.6 Sol** from the model selector in the chat and enter your request normally. For example:

> Analyze the frontend and backend architectures independently, identify risks, and provide a consolidated recommendation. Do not modify files.

The skill can be invoked implicitly when Sol determines that delegation will materially improve speed, quality, or context isolation. Because this is a model decision, a simple task may be completed without subagents. This is expected behavior.

## Request orchestration explicitly

You do not need to mention the skill in every prompt. When you want to ensure it is used for a specific task, write:

> Use `$dcc-orchestration` and delegate the independent parts to Terra agents.

You can also request delegation without naming the skill:

> Delegate the frontend and backend analysis to separate Terra agents and consolidate their results. Do not modify files.

## Configure concurrent subagents

The concurrency setting is under **Settings → Providers → Codex → Concurrent subagents**. It applies only to new sessions.

- **Automatic (Codex):** DCC does not send a limit and lets Codex decide.
- **Numeric limit:** restricts how many subagents may be open at the same time, excluding Sol.

The preset starts the smallest useful team, normally one to three subagents, and respects the configured limit.

## Quick test: verify Sol → Terra

This test verifies the delegation infrastructure without modifying files:

1. Install the preset by following the steps above.
2. Create a new session.
3. Select **GPT-5.6 Sol** in the chat.
4. Send this prompt:

   > Use `$dcc-orchestration`. Start two Terra agents in parallel: one should analyze only the frontend architecture and the other only the backend architecture. Do not modify files. Wait for both agents and provide a consolidated conclusion.

5. Expand **Agent tree** in the response.

During execution, the expected result is:

- **Primary agent — GPT-5.6 Sol**;
- two Terra subagents with their own names and paths;
- **Working** while each agent is running;
- **Completed** or **Failed** when each agent finishes;
- the chat becomes available after Sol completes the final response.

In older sessions that did not record a subagent's final event, DCC may display **Ended**. This is a neutral historical state that prevents the UI from incorrectly claiming that the agent is still working.

## Test automatic activation

After the explicit test succeeds, create another Sol session and send a complex request without mentioning delegation or the skill:

> Independently evaluate the frontend architecture, backend architecture, and test strategy. Compare the results and recommend the three most important improvements. Do not modify files.

If Sol determines that the workstreams are sufficiently independent, the agent tree will show the subagents it selected automatically. The absence of subagents for a small request is not a failure. Use the explicit quick test to distinguish a model decision from an infrastructure problem.

## Monitor and supervise subagents

In the **Agent tree**, each row represents a subagent started by Sol. While a subagent is active, you can:

- use **Instruct** to send additional guidance;
- use **Interrupt** to request that the subagent stop;
- monitor the model and status of each agent.

Sol remains the primary agent and integrates the work. A subagent does not replace the primary agent or complete the primary session by itself.

## If no subagent appears

Check the following, in order:

1. **GPT-5.6 Sol** is selected in the chat.
2. The button under **Skills** displays **Orchestration installed**.
3. The skill includes **Codex** as a target agent.
4. **Disable model invocation** is turned off for the automatic activation test.
5. You created a new session after saving the skill or changing concurrency.
6. You used the explicit prompt from the quick test.

DCC detects whether the installed Codex CLI supports the orchestration used by the session. When the capability is unavailable, DCC preserves the previous behavior instead of breaking the session. In that case, update the Codex CLI when a compatible version is available and repeat the test in a new session.

## Summary

- The preset teaches Sol when and how to delegate.
- Automatic activation does not require `$dcc-orchestration` in the prompt.
- Naming `$dcc-orchestration` explicitly invokes the skill for that task.
- Select the model in the chat; configure concurrency under **Settings → Providers**.
- Test skill installation and concurrency changes in a new session.

