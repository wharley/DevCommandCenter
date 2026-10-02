# DCC agents

A DCC agent has a fixed role and steps in at a defined point of your workflow.
It is tied to DCC on three sides: what it reads, when it acts, and where its
result goes. The first one is the **Reviewer**.

DCC agents are different from [delegation](DELEGATION_AGENTS.md), where you hand
a one-off task to another provider, and from [skills](SKILL_PRESETS.md), which
are instructions the provider loads on its own.

## The Reviewer

The Reviewer reads the changes of a task and reports problems it can point to in
the code: correctness bugs, unhandled edge cases, missing error handling,
regressions, security issues, and new behaviour without a test. It does not edit
files.

### In a task

1. When a turn ends and the task has local changes, the Reviewer's mascot
   appears above the composer and asks whether it may review. Nothing runs until
   you accept. You can also call it from its icon in the task header, which is
   only active when there is something to review and no turn is running.
2. It opens its own conversation in the same task, so it sees the same
   checkout. Its role is sent with every turn of that conversation.
3. Its answer ends with a list of findings, ordered by severity, each with a
   file and line. Selecting a finding opens the file's diff at that line.
4. Send one finding, or all of them, to the author agent. DCC switches to the
   task's most recently used ordinary conversation and drafts a fix request in
   its composer, carrying the Reviewer's full explanation. You read it and send.

Findings are also marked on the diff: in the Inspector's changes, in the file
surface, and in the changes of the latest execution.

Once the author finishes another turn, the review is outdated. The list stays
readable but stops acting, and the markings are removed, because the lines the
Reviewer pointed at have moved. Run the Reviewer again to check the fix.

### In Pull Requests

On the code tab of a pull request, the Reviewer reviews from the patch and
leaves editable draft comments. On a pull request waiting for your review it
offers to do so.

The review keeps running if you leave the pull request. The Agents section of
the sidebar shows when it is working and when a result is ready, and the
Reviewer's page lists its pull request reviews. Drafts are restored when you
return, until you submit the review.

Limits of a pull request review:

- It only sees the patch. A pull request from someone else has no worktree, so
  the Reviewer cannot read the rest of the repository or run anything.
- A comment can only be placed on a line the patch shows.
- Reviews in progress and unsent drafts are kept in memory. Quitting DCC
  discards them, and edits to a draft are lost if you leave before submitting.
- Nothing is published until you submit the review.

## What you can change

Open the Reviewer from the Agents section of the sidebar and select Edit:

- **Name and mascot** (colour, arms and eyes).
- **Provider and model.** By default it uses the ones selected in the composer.
  Choosing a different provider from the author's gives an independent read.
- **Extra instructions**, added to the role on every turn: your project's
  conventions, or folders to ignore.
- **Offer after a turn.** Turn it off and the Reviewer only runs when you call
  it.

The role itself is maintained by DCC and improves with each release. It is in
[`crates/dcc-core/src/domain/agent.rs`](../crates/dcc-core/src/domain/agent.rs),
and the Reviewer's page shows it on request.

## State

The mascot shows what the agent is doing: resting, working (it bobs), blocked
on you (a raised arm and an amber badge), or with a result you have not opened
(a smile and a green badge). The sidebar shows the most urgent state across all
of the agent's work, in every project.

## Cost and control

The Reviewer runs on your own provider account, like any other conversation.
It never starts by itself: it offers, and you accept. Read-only is an
instruction in its role, not an enforced sandbox; permission requests follow the
access mode selected in the composer.

## Local checks

```sh
cargo test -p dcc-core agent --lib
cargo test -p dcc-infra resident_agents --lib
yarn workspace @dcc/desktop test src/features/agents
```
