# Timeline provider conformance

The DCC timeline is a provider-neutral projection, not a lowest-common-denominator
text stream. Adapters must preserve every reliable lifecycle signal exposed by
their provider and use synthesis only for information the provider does not
publish.

## Canonical assistant-message contract

Rich adapters emit:

1. `AssistantMessageStarted` with a stable provider item/message ID.
2. `AssistantMessageDelta` for incremental text.
3. `AssistantMessageCompleted` with the same ID and, when available, the
   provider's authoritative final text snapshot.

`AssistantMessageCompleted.content` replaces accumulated deltas. This makes a
dropped, duplicated, or partially delivered chunk recoverable at completion.

Providers without message lifecycle continue to emit `TextDelta`. The Tauri
normalizer creates a stable synthetic item and closes it at semantic boundaries
such as reasoning, tool calls, permissions, user input, and turn completion.

## Tool-call contract

1. `ToolCallStarted` with the provider's call ID, tool name and whatever
   command/file is already known.
2. `ToolCallDelta` carries streamed **output** only (stdout, progress text).
   Raw tool input is never sent as a delta: Claude's `input_json_delta`
   fragments are buffered in the adapter.
3. `ToolCallUpdated { detail }` when structured facts become known mid-flight,
   for example the command or edit diff once Claude's streamed input closes.
4. `ToolCallCompleted` / `ToolCallFailed` with an optional `detail` holding the
   result: output text, exit code and unified diff.

`ToolCallDetail` fields are all optional and merge (later non-empty fields win).
Every adapter bounds them through `ToolCallDetail::bounded`: input 8K chars,
output 8K head + 16K tail, diff 64K. Sources:

| Provider path | Command / file | Output | Exit code | Diff |
| --- | --- | --- | --- | --- |
| Claude Agent SDK | Buffered tool input at block stop | `tool_result` content; Bash `stdout`/`stderr` | — | Edit/MultiEdit/Write input, then SDK `structuredPatch` |
| Codex app-server | `item/started` | `item/commandExecution/outputDelta`, then `aggregatedOutput` | `exitCode` | `fileChange.changes[].diff` |
| ACP (Cursor, Grok, Antigravity) | `rawInput` | `content` text blocks or `rawOutput` | `rawOutput.exitCode` | `content` diff blocks |

## User words vs. DCC context

A turn carries the user's prompt plus DCC context: composer directives
(plan/fast), and `tool_instructions` (delegation rules, objective, skill and
memory context, MCP routing, cold-attach snapshot). When the provider has a
separate channel, the context goes there. The provider's native history then
holds only what the user wrote.

| Provider path | DCC context channel | Text the provider stores as the user turn |
| --- | --- | --- |
| Claude Agent SDK | `systemPrompt.append` (sidecar `buildSystemPrompt`) | Prompt only |
| Codex app-server ≥ 0.135.0 | `turn/start.additionalContext`, `kind: "application"` (developer role, experimental API) | Prompt only |
| Codex app-server < 0.135.0 or unknown version | Directive block + `[DCC provider tool instructions]` in the text | Prompt + context |
| ACP (Cursor, Grok, Antigravity), stream-json (Gemini, Droid, Cursor) | Text fallback (`append_tool_instructions`) | Prompt + context |

Codex details (`codex_turn_payload` in `crates/dcc-providers/src/codex_app_server.rs`):

- `additionalContext` landed in codex-cli 0.135.0 (openai/codex#24154). Older
  app-servers ignore the field, so the adapter gates on the version from
  `codex --version` and keeps the text fallback below it.
- Codex truncates the middle of each entry past 1,000 tokens (4 bytes per
  token). Instructions are split at line boundaries into ≤ 4,000-byte entries
  (`dcc_instructions_001`, `_002`, …). The directives go in `dcc_directives`.
  Codex orders entries by key.
- Codex re-injects an entry only when its value changes, and never resets that
  memory on compaction. When a root-thread `contextCompaction` item completes,
  DCC sends the last context back with `thread/inject_items` as the same
  `<key>value</key>` developer messages, even mid-turn (T3 Code does the
  same). A fresh runtime (including native resume) starts with an empty store
  and injects everything on its first turn.
- `thread/start.developerInstructions` is thread-scoped and
  `collaborationMode` replaces Codex's built-in mode instructions, so neither
  fits per-turn context.

## Streaming persistence

The Tauri bridge merges consecutive deltas of the same item that arrive within
50 ms (`provider_delta_coalescer`) before they reach the durable log. Any other
event closes the window immediately, so ordering and terminal latency are
unchanged.

## Current provider matrix

| Provider path | Identity | Streaming | Authoritative completion | Phase | Notes |
| --- | --- | --- | --- | --- | --- |
| Codex app-server | Native item ID | Native item delta | Native `item/completed.text` | Native `commentary` / `final_answer` | Highest-fidelity reference adapter |
| Claude Agent SDK | Native API message ID | Raw partial message events | Complete `AssistantMessage.message.content` | Inferred | Root messages only; subagent text is not flattened |
| Gemini stream-json | Stable DCC turn-message ID | Assistant message chunks | Terminal result/response | Inferred | CLI does not expose a message ID in the documented stream |
| Droid stream-json | Native message ID when present | Message text events | `completion.finalText` / result | Inferred | Missing IDs retain the semantic-boundary fallback |
| Cursor ACP | Native ACP `messageId` when present | `agent_message_chunk` | Turn boundary | Inferred | Older ACP agents without IDs retain the fallback |
| Cursor stream-json | Stable DCC turn-message ID | Assistant deltas | Terminal `result` | Inferred | Cursor documents result as the accumulated assistant response |
| Grok ACP | Native ACP `messageId` when present | `agent_message_chunk` | Turn boundary | Inferred | Older ACP agents without IDs retain the fallback |

## Adapter rules

- Never concatenate separate native message IDs into one item.
- Never discard an authoritative final snapshot merely because deltas were seen.
- Never flatten subagent messages into the root transcript without explicit
  provenance and a nested timeline model.
- Treat provider IDs as opaque, bounded strings.
- A delta without a start is valid input: the core synthesizes only the missing
  start edge while retaining the provider ID.
- A completion without a start is valid input: history projection creates the
  item from the authoritative completion.
- Unknown protocol fields and event variants must be ignored safely.
- New provider versions require fixture coverage for text, tools, reasoning,
  failure, interrupted streams, and authoritative completion before increasing
  their conformance claim.

## Phase semantics

Only Codex currently publishes an explicit assistant-message phase. Other
providers remain `Unknown`; after the turn settles, the timeline selects the
explicit `final_answer` when present, otherwise the last non-empty assistant
message. Earlier assistant messages are retained as commentary annotations,
not deleted.

While a turn runs, commentary is rendered inline in execution order between
bursts of work, so text is visible as it streams for every provider. Once the
turn settles, everything except the final answer folds behind one
"Worked for …" row with a one-line summary of the work.
