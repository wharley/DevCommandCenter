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
