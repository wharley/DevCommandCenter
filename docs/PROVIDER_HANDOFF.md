# Provider Handoff

Provider handoff keeps an existing DCC task oriented when its next turn uses a
different provider. There is no separate handoff command: select another
provider in the composer and send the next message normally.

## How It Works

1. Continue in a task that already has conversation history.
2. Select a different provider in the composer. DCC shows a toast explaining
   that limited context will follow on the next turn.
3. Write and send the next message normally.
4. DCC detects the provider change and attaches one bounded, provider-neutral
   re-anchor to that turn.
5. Later turns with the same provider continue normally and do not receive the
   handoff packet again.

The provider selection itself does not start a runtime or send content. The
handoff happens when the next direct turn is sent. It does not create a child
agent, start a delegation, or move the task to another worktree.

## Context That Follows

The re-anchor is assembled locally from durable DCC and workspace state. When
available, it contains:

- source and destination provider identifiers;
- workspace path, branch, and a bounded Git change summary;
- the mission specification and active plan;
- recent completed user and assistant messages.

The current user message remains the instruction the new provider must follow.
The re-anchor is labeled as background context, not as a new instruction, and
the current message is removed from recent history so it is not sent twice.

## Boundaries

Provider handoff is a practical re-anchor, not native 1:1 memory transfer
between provider runtimes. It intentionally does not send:

- the complete transcript;
- hidden reasoning;
- tool-call noise or tool results;
- streaming, incomplete, system, or status messages;
- raw Git patches.

The complete handoff is currently capped at 12,000 characters. Recent messages
are selected newest-first within their budget so the most relevant part of the
conversation survives when the session is long.

Starting a new DCC task or thread starts fresh. A handoff is only created for a
direct turn in an existing session with history when the selected provider is
different from the provider that handled the preceding turn. Delegation uses
its own explicit parent-to-child context flow.

If optional Git or mission data cannot be loaded, DCC still sends the available
conversation context. If the handoff itself cannot be assembled, the user's
turn proceeds without it instead of being blocked.
