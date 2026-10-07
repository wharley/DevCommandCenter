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
| Cursor stream-json | `tool_call.<x>ToolCall.args` (`command`, `path`) | Result `interleavedOutput`, else `stdout` + `stderr`; Read `content`; MCP `content[].text.text`; error/reason text | Shell `success`/`failure.exitCode` | Edit `diffString`; Write `args.fileText`; Delete `prevContent` |
| Droid stream-json | Matching `tool_call.parameters` (kept by id) | `tool_result.value` | Not exposed | Edit/MultiEdit `old_str`/`new_str`, Create `content`, ApplyPatch `input` envelope |
| Gemini stream-json | Matching `tool_use.parameters` (kept by `tool_id`) | `tool_result.output` (string displays only) | Not exposed | `replace` `old_string`/`new_string`, `write_file` `content`, on success only |
| Generic CLI envelope | `tool_call_started` | Optional `output` on `tool_call_completed`/`tool_call_failed` | Optional `exit_code` | Optional `diff` |

Provider-specific notes:

- **Cursor stream-json** (`parse_cursor_tool_call` in `cursor.rs`). Shapes
  come from cursor-agent 2026.10.01: `tool_call` is a serialized protobuf
  message, so the tool is a oneof key (`shellToolCall`, `editToolCall`, …)
  next to metadata like `toolCallId` and `hookAdditionalContexts`. The adapter
  picks the `…ToolCall` object, not the first key. Results are serialized with
  default values, so a shell `exitCode` of 0 is present. Any result variant
  other than `success` (`failure` with a non-zero exit, `rejected`, `error`,
  `permissionDenied`, …) becomes `ToolCallFailed`, the same rule as Codex.
- **Droid stream-json** (`droid_tool_detail` in `droid.rs`). `tool_result`
  carries only `id`, `toolId`, `isError` and `value`, so the command and edit
  diff come from the `tool_call` parameters. `Execute` results are plain text
  with no exit code. The output used to be streamed as an unbounded
  `ToolCallDelta`; it now travels once, bounded, in the completion detail.
- **Gemini stream-json** (`gemini_tool_detail` in `headless_cli.rs`, gemini-cli
  0.32.1). `tool_result.output` is the tool's `resultDisplay` only when it is a
  string. Edits display a diff object the stream drops, so the diff is rebuilt
  from the `tool_use` parameters (no line numbers). Shell exit codes are not
  in the stream.
- **Generic CLI envelope** (`ProviderEnvelope` in `common.rs`): DCC's own
  `tool_call_completed` / `tool_call_failed` lines accept optional `command`,
  `file`, `output`, `exit_code` and `diff`. Lines without them still parse.

## Turn usage

`ProviderEvent::TurnUsage` feeds the per-turn footer (`session_turn_usage`).
DCC stores one row per turn and model and replaces it on every report, so a
provider that reports cumulative per-turn totals once at the end and one that
reports several snapshots behave the same. `input_tokens` excludes cache reads
and writes in every adapter except Codex, whose `inputTokens` include cached
input (OpenAI convention); `total_tokens` is the provider's own total when it
publishes one. Cost is never estimated.

| Provider path | Source | Model | Notes |
| --- | --- | --- | --- |
| Claude Agent SDK | `result.modelUsage` (per model), else `result.usage` | Per model | Includes `costUSD` when present |
| Codex app-server | `thread/tokenUsage/updated.tokenUsage.last` | — | Includes reasoning output tokens |
| Cursor stream-json | `result.usage` (`inputTokens`, `outputTokens`, `cacheReadTokens`, `cacheWriteTokens`) | `system/init.model` (display name) | cursor-agent already subtracts cache tokens from `inputTokens` |
| Droid stream-json | `completion.usage` (also `result.usage`) | `system/init.model` | Anthropic field names, but `input_tokens` includes cache reads (a real capture shows 21,960 input with 21,693 cache reads); DCC subtracts them. `factory_credits` is ignored |
| Gemini stream-json | `result.stats` (`input`, `cached`, `output_tokens`, `total_tokens`) | `init.model` | Process-wide metrics, which cover exactly one turn because DCC runs one `gemini` process per turn. Thought and tool-prompt tokens are only inside `total_tokens` |
| ACP (Cursor, Grok, Antigravity) | `session/prompt` response `usage` (`inputTokens`, `outputTokens`, `cachedReadTokens`, `cachedWriteTokens`, `thoughtTokens`, `totalTokens`) | — | Read when present. cursor-agent 2026.10.01 and Grok 0.2.101 answer with `stopReason` only, so they report nothing today. ACP `usage_update` is context-window occupancy (`used`/`size`), not per-turn tokens, and is not used |

## Native image input

The composer serializes pasted and attached images as `@/absolute/path.png`
tokens. `referenced_image_paths` (`common.rs`) picks existing files (png, jpg,
jpeg, gif, webp; at most 8, 20 MB each). The textual reference always stays
in the prompt, so a provider without image input can still open the file
with a tool.

| Provider path | Native image input |
| --- | --- |
| Claude Agent SDK | Image content blocks |
| Codex app-server | `localImage` input items |
| ACP (Cursor, Grok, Antigravity) | `image` content blocks (`mimeType`, base64 `data`, `file://` `uri`) in `session/prompt`, only when `initialize` advertised `agentCapabilities.promptCapabilities.image` (cursor-agent 2026.10.01 does) |
| Cursor / Droid / Gemini stream-json | None: the prompt is a single CLI argument with no attachment channel |

## Native resume

A runtime that restarts (Stop, app restart) reattaches to the provider's own
conversation when the adapter reports `supports_native_resume`. The bridge
persists the id from `NativeSessionChanged` per DCC session and provider, and
holds the cold-attach history snapshot back as `resume_fallback_context`; the
adapter sends it only if the provider refused the id.

| Provider path | Resume channel | On refusal |
| --- | --- | --- |
| Claude Agent SDK | `DCC_RESUME_SESSION_ID` → SDK `resume` | Sidecar forgets the id and retries the turn fresh with the snapshot |
| Codex app-server | `thread/resume { threadId, excludeTurns: true }` with the same cwd, sandbox, workspace roots and MCP `config` as `thread/start` | `thread/start`, and the snapshot joins the first turn's DCC context |
| Cursor / Droid / Gemini CLIs | CLI-native continuation (`--resume`, `--session-id`) within one runtime only | — |
| ACP (Cursor, Grok, Antigravity) | Not implemented | — |

Codex details (`open_thread` in `codex_app_server.rs`): `thread/resume` is
stable in the v2 app-server; DCC still tries it on every version and falls
back on any error, so an app-server without it behaves like a fresh start.
The thread id is reported on the first input of the runtime, once the event
stream is subscribed. `thread/resume` does not accept `experimentalRawEvents`,
so on a resumed thread the model a native subagent was asked to spawn with is
not shown until Codex confirms it. Codex's `additionalContext` memory is per
process, so the first turn after a resume re-sends every context entry.

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

## Steering an active turn

Steering adds the user's guidance to the running turn instead of queueing it.
`steer_turn` calls the adapter first and appends `TurnSteered` only after the
provider accepts the text. The timeline renders that event as a user bubble
for every provider. An adapter that can tell the turn is already finishing
refuses with an error starting with `STEER_WINDOW_CLOSED`
(`dcc_core::ports`). The composer then queues the same text as the next
message. Delegation hand-backs queue on any steer error.

| Provider path | Steering channel | When it is refused |
| --- | --- | --- |
| Codex app-server | `turn/steer { threadId, input, expectedTurnId }` | No active turn id (error, not queued) |
| Claude Agent SDK | Extra `user` message on the turn's open SDK prompt stream, `priority: "next"` | Before Claude reports `running`, after the turn's `result`, or when the turn input has ended |
| ACP, stream-json CLIs | None: the composer offers the queue only | — |

Claude details (`sidecar/src/turn-input.mjs`, `steer` in `claude_sdk_sidecar.rs`):

- The SDK `prompt` is an async iterable that stays open while the turn runs.
  Rust writes `{"type":"steer","requestId","prompt"}` to the sidecar's stdin.
  The sidecar answers `dcc_steer_result { request_id, accepted }` on stdout.
  Rust waits up to 10 s for the answer and never forwards it as a provider event.
- The sidecar sets `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1`, so Claude
  reports `system/session_state_changed` (`running`, `requires_action`,
  `idle`). The sidecar consumes these messages and does not forward them.
  Steering is offered only after Claude reports `running`, which proves that
  Claude will also report `idle`. A Claude Code that never reports state never
  steers, and its turn ends at the `result` exactly as before.
- `priority: "next"` lets Claude Code fold the guidance in at its next tool
  boundary ("address this before completing your current task"). If the turn
  had no tool boundary left, Claude runs the guidance as a follow-up in the same
  process. That follow-up reports `running` again and can be steered too. The
  last `result` closes the DCC turn. `modelUsage` is cumulative per process,
  so that `result` covers the whole turn.
- An unsteered turn ends its input at the `result`. A steered turn ends it at
  `idle`, after a 1 s settle window. A steer written just as Claude went idle
  starts as a follow-up within that window. If Claude never reports `idle`
  after a steered `result`, the input ends 15 s later.
- Referenced images (`@/abs/path.png`) in the steer text become image blocks,
  as in a turn prompt. Steers carry no DCC context: they are user words only.
- Native resume: a refused resume fails before `system/init` and before
  `running`, so no guidance is accepted by an attempt that the sidecar then
  retries fresh. With DCC MCP servers, the first message waits for them to
  attach, and steering waits with it.
- Not verified against a live Claude Code. The protocol was read from the
  Agent SDK 0.2.126 types and the Claude Code 2.1.287 binary.

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
