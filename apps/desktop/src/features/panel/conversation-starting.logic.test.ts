import { describe, expect, it } from "vitest";
import type { WorkspaceMessage } from "./thread-projection";
import { shouldShowConversationStarting } from "./conversation-starting.logic";

function message(
	id: string,
	role: WorkspaceMessage["role"],
	turnId?: string,
): WorkspaceMessage {
	return { id, role, label: role, content: id, turnId };
}

describe("shouldShowConversationStarting", () => {
	it("shows while an optimistic prompt is being accepted", () => {
		expect(
			shouldShowConversationStarting(
				[message("assistant-1", "assistant"), message("pending-user", "user")],
				"Next prompt",
				null,
			),
		).toBe(true);
	});

	it("continues after the running turn is accepted but has no activity", () => {
		expect(
			shouldShowConversationStarting(
				[message("user-1", "user", "turn-1")],
				null,
				"running",
			),
		).toBe(true);
	});

	it("hides as soon as assistant activity appears", () => {
		expect(
			shouldShowConversationStarting(
				[
					message("user-1", "user", "turn-1"),
					message("assistant-1", "assistant", "turn-1"),
				],
				null,
				"running",
			),
		).toBe(false);
	});

	it("does not treat a steer message as a newly starting turn", () => {
		expect(
			shouldShowConversationStarting(
				[
					message("user-1", "user", "turn-1"),
					message("assistant-1", "assistant", "turn-1"),
					message("steer-1", "user"),
				],
				null,
				"running",
			),
		).toBe(false);
	});
});
