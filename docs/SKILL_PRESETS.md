# DCC skill presets

The orchestration preset coordinates native subagents of the active provider.
It does not enable cross-provider delegation, switch the selected model, or
change the session's Local/Worktree environment. When the runtime does not
expose suitable subagent tools, the primary agent continues the task itself.

## Provider targets

| Target | Generated project file | Invocation |
| --- | --- | --- |
| Claude | `.claude/skills/<name>/SKILL.md` | Native skill; explicit-only preference is written to frontmatter. |
| Codex | `.agents/skills/<name>/SKILL.md` | Native skill; invocation policy is written to `agents/openai.yaml`. |
| Gemini | DCC block in `GEMINI.md` | Instructions included in project context; disabled skills are omitted. |
| Cursor | `.cursor/rules/<name>.mdc` | Rule selected by relevance (`alwaysApply: false`); disabled skills are omitted. |
| Grok | `.grok/skills/<name>/SKILL.md` | Native skill; explicit-only preference is written to frontmatter. |

These are DCC's current compilation formats, not a claim that all providers
have identical skill discovery or subagent capabilities. Compiled instructions
are written into the active checkout. A running provider may need a new turn
or session to discover updated files.

Droid's legacy `agents` target is no longer offered for new selections. Existing
records remain readable and continue compiling their DCC-owned `AGENTS.md`
block until the user removes the legacy link. Handwritten content is preserved.

## Updating an existing project copy

Open **Skills → DCC catalog → Review update**. This opens the current preset's
description and instructions in the editor. Existing provider selections and
the invocation preference are preserved. Review the text, select any additional
providers, and save to replace the project copy and compile it.

Opening or cancelling this review does not change the saved skill. Preset
updates never silently overwrite customized project instructions.

## Orchestration behavior

- Use only native tools and roles exposed in the current session.
- Honor explicit model choices; prefer suitable lower-cost native models for bounded supporting work when available, with configured defaults as the fallback.
- Keep complex decisions and final review with the primary agent; weigh delegation overhead against its benefit without assuming savings.
- Give independent tasks bounded context and disjoint write scopes.
- Preserve the active checkout and existing changes; serialize overlapping work.
- Review results before starting dependent work or declaring success.
- Stop obsolete workers when possible and report failures or missing controls.

Validation covers compilation, invocation preferences, checkout placement, the
editor workflow, and preservation of external skills. It does not certify
live subagent execution on every provider/version.

Provider references:

- [Claude subagents](https://code.claude.com/docs/en/sub-agents)
- [Gemini CLI subagents](https://geminicli.com/docs/core/subagents/)
- [Cursor subagents](https://cursor.com/docs/subagents)
- [Grok skills](https://docs.x.ai/build/features/skills-plugins-marketplaces)
- [Grok subagents](https://docs.x.ai/build/features/subagents)

## Next: shared memory

Evaluate ai-memory capture and retrieval through DCC's own provider launch
paths before choosing an integration. The first acceptance scenario is a
Claude session recording decisions and unfinished work, followed by a new
Codex session recovering that context without confusing Local/Worktree state.
Native orchestration remains separate from that memory work.
