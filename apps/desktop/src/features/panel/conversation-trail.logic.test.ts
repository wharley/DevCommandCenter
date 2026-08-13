import { describe, expect, it } from "vitest";
import type { WorkspaceMessage } from "./thread-projection";
import {
	deriveConversationTrailItems,
	focusedTrailIndex,
	trailMagnificationWeights,
} from "./conversation-trail.logic";

function message(overrides: Partial<WorkspaceMessage>): WorkspaceMessage {
	return {
		id: "message-1",
		role: "assistant",
		content: "",
		label: "Assistant",
		...overrides,
	};
}

describe("conversation trail", () => {
	it("creates one stop per user turn and keeps the final assistant preview", () => {
		const items = deriveConversationTrailItems([
			message({ id: "user-1", role: "user", turnId: "turn-1", content: "  First\n request " }),
			message({ id: "assistant-1", turnId: "turn-1", content: "Starting work" }),
			message({ id: "user-2", role: "user", turnId: "turn-2", content: "Second request" }),
			message({ id: "assistant-2a", turnId: "turn-2", content: "Checking" }),
			message({ id: "assistant-2b", turnId: "turn-2", content: "Finished successfully" }),
		]);

		expect(items).toEqual([
			{
				id: "user-1",
				ordinal: 1,
				promptPreview: "First request",
				responsePreview: "Starting work",
			},
			{
				id: "user-2",
				ordinal: 2,
				promptPreview: "Second request",
				responsePreview: "Finished successfully",
			},
		]);
	});

	it("falls back to transcript order when legacy messages have no turn id", () => {
		const items = deriveConversationTrailItems([
			message({ id: "user-1", role: "user", content: "Legacy request" }),
			message({ id: "assistant-1", content: "Legacy response" }),
		]);

		expect(items[0]?.responsePreview).toBe("Legacy response");
	});

	it("focuses the closest tick and applies a gaussian falloff", () => {
		expect(focusedTrailIndex(19, 5, 10)).toBe(2);
		expect(focusedTrailIndex(-20, 5, 10)).toBe(0);
		expect(focusedTrailIndex(99, 5, 10)).toBe(4);

		const weights = trailMagnificationWeights(5, 20, 10, 14);
		expect(weights[2]).toBe(1);
		expect(weights[1]).toBeCloseTo(weights[3]!, 8);
		expect(weights[0]!).toBeLessThan(weights[1]!);
	});
});
