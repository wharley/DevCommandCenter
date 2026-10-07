# Edit from here

**Edit from here** (pt-BR: **Editar a partir daqui**) is the pencil on your own
messages in the conversation. It takes the conversation back to just before
that message. Then it puts the message, with its attachments, back in the
composer so you can change it and send it again.

DCC decides everything before anything changes. A dialog then shows you three
things:

- how many turns leave the conversation (the chosen message and every later
  one);
- how the agent's own memory follows;
- whether the files those turns changed can be restored.

Closing or cancelling the dialog changes nothing.

## The agent's memory

| What DCC can do | What happens |
| --- | --- |
| **Cut the native conversation** (Claude Code, Codex) | The agent forgets the removed turns. Claude resumes at the last kept message (`resumeSessionAt`, forked into a new native session). Codex drops the removed turns from its thread (`thread/rollback`). Either way, the cut happens when the next message is sent. |
| **Nothing is kept** (you edit the first message) | The agent starts a new conversation. |
| **Cannot cut** (other providers, or turns DCC holds no cut point for) | The work continues in a **new thread**. That thread is re-anchored on a bounded summary of the messages before the chosen one, as **Fork** does. The current thread stays as it is, because its agent still remembers everything. |

DCC records a cut point for every turn as it happens: the last assistant
message for Claude, the turn id for Codex. Turns from before this feature have
no cut point, so editing from one of them continues in a new thread.

If the provider cannot make the recorded cut when the next message is sent,
DCC does not resume the uncut conversation. It starts fresh with the bounded
summary of the kept messages instead. Two examples: the native transcript is
gone, or Codex no longer holds that turn.

## Files

Files are restored only by [Guarded Undo](GUARDED_UNDO.md), and only when you
choose **Go back and restore _n_ files**. **Go back without restoring files**
never touches a file.

DCC restores the removed turns newest first. Each turn gets its own Guarded
Undo preparation and verification. Before offering the restore, DCC checks
every turn read-only:

- each changed file must still be exactly what that turn left. If a newer
  removed turn also changed it, that newer turn must have started from exactly
  those bytes and permissions;
- every removed turn that changed files must be protected by Guarded Undo.

When several removed turns changed the same file, it is restored turn by turn,
and the dialog lists it once. After a newer turn's restore, the older turn
accepts only the exact file that restore put back. A file with the same content
written by anything else is refused. See
[chained restore](GUARDED_UNDO_DESIGN.md#chained-restore-after-a-completed-undo).

The restore is not offered, and files stay as they are, when:

- something other than the removed turns changed a file after the last
  removed turn that changed it: you, an editor, or another task in the same
  workspace;
- something changed a file between two removed turns that both changed it;
- a turn was not protected. This includes new or deleted files, untracked
  paths, Git index or HEAD changes, platforms without Guarded Undo, and
  expired captures.

Changes made outside the removed turns are never restore targets.

If something changes between your confirmation and the restore, restoring
stops at that turn. Turns already restored stay restored, the dialog says which
ones, and the conversation is not rewound.

## What stays

The removed turns stay in DCC's durable history for audit, behind a
`conversation_rewound` marker. The timeline, search, the sidebar summary and
every re-anchor sent to an agent skip them.

Delegations that removed turns started, and commits, pushes or other effects
outside the files, are not undone.
