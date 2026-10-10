import { describe, expect, it } from "vitest";
import { applyEvents, createThreadState, type RawSessionEvent } from "./threadEvents.ts";
import { agentTurnHeads, groupMessages, isThreadRunning, threadTitle } from "./timeline.ts";

let seq = 0;
function ev(type: string, extra: Record<string, unknown> = {}): RawSessionEvent {
	seq += 1;
	return { eventId: `e${seq}`, sessionId: "s", sequence: seq, occurredAt: "2026-10-09T00:00:00Z", kind: { type, ...extra } };
}

describe("timeline", () => {
	it("folds machinery between the prompt and the answer into one trace", () => {
		const state = applyEvents(createThreadState(), [
			ev("turn_started", { turnId: "t", prompt: "  conserte   o build " }),
			ev("turn_reasoning_started", { turnId: "t", reasoningId: "r" }),
			ev("turn_tool_call_started", { turnId: "t", toolCallId: "c", toolName: "Bash" }),
			ev("turn_permission_requested", { turnId: "t", requestId: "p" }),
		]);
		const items = groupMessages(state.messages);
		expect(items.map((i) => i.type)).toEqual(["single", "trace", "single"]);
		expect(isThreadRunning(state.messages)).toBe(true);
		expect(threadTitle(state.messages)).toBe("conserte o build");

		const done = applyEvents(state, [
			ev("turn_permission_resolved", { turnId: "t", requestId: "p" }),
			ev("turn_tool_call_completed", { turnId: "t", toolCallId: "c" }),
			ev("turn_reasoning_completed", { turnId: "t", reasoningId: "r" }),
			ev("turn_delta", { turnId: "t", content: "feito" }),
			ev("turn_completed", { turnId: "t" }),
		]);
		const folded = groupMessages(done.messages);
		expect(folded.map((i) => i.type)).toEqual(["single", "trace", "single"]);
		expect(folded[1]?.type === "trace" && folded[1].steps.length).toBe(3);
		expect(isThreadRunning(done.messages)).toBe(false);
	});

	it("marks the first agent item after each prompt as a turn head", () => {
		const state = applyEvents(createThreadState(), [
			ev("turn_started", { turnId: "a", prompt: "um" }),
			ev("turn_tool_call_started", { turnId: "a", toolCallId: "c1", toolName: "Read" }),
			ev("turn_tool_call_completed", { turnId: "a", toolCallId: "c1" }),
			ev("turn_delta", { turnId: "a", content: "pronto" }),
			ev("turn_completed", { turnId: "a" }),
			ev("turn_started", { turnId: "b", prompt: "dois" }),
			ev("turn_delta", { turnId: "b", content: "ok" }),
		]);
		const items = groupMessages(state.messages);
		const heads = agentTurnHeads(items);
		expect(heads).toHaveLength(2);
		expect(heads[0]).toBe(items[1]?.key);
		expect(heads[1]).toBe(items.at(-1)?.key);
		// A prompt still waiting for output has no head yet.
		const pending = applyEvents(state, [ev("turn_completed", { turnId: "b" }), ev("turn_started", { turnId: "c", prompt: "três" })]);
		expect(agentTurnHeads(groupMessages(pending.messages))).toHaveLength(2);
	});
});
