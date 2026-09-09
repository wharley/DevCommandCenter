import { describe, expect, it } from "vitest";
import type { WorkspaceMessage } from "./thread-projection";
import { conversationStartingPhase, shouldShowConversationStarting, shouldShowInitialConversationStarting } from "./conversation-starting.logic";

function message(
	id: string,
	role: WorkspaceMessage["role"],
	turnId?: string,
): WorkspaceMessage {
	return { id, role, label: role, content: id, turnId };
}

describe("shouldShowConversationStarting", () => {
	it("keeps the first prompt behind the steps from creation through acceptance", () => {
		expect(shouldShowInitialConversationStarting([], "First prompt", null)).toBe(true);
		expect(shouldShowInitialConversationStarting([message("pending", "user")], "First prompt", null)).toBe(true);
		expect(shouldShowInitialConversationStarting([message("user", "user", "turn-1")], null, "running")).toBe(true);
	});

	it("reveals the first prompt alongside agent activity or a failure", () => {
		const user = message("user", "user", "turn-1");
		expect(shouldShowInitialConversationStarting([user, message("reply", "assistant")], "First prompt", "running")).toBe(false);
		expect(shouldShowInitialConversationStarting([user, { ...message("error", "system"), label: "session.aborted" }], "First prompt", "aborted")).toBe(false);
		expect(shouldShowInitialConversationStarting([], null, null)).toBe(false);
	});

	it("preserves the existing conversation while a follow-up starts", () => {
		const messages = [message("previous", "user", "turn-1"), message("answer", "assistant"), message("pending", "user")];
		expect(shouldShowInitialConversationStarting(messages, "Next prompt", "running")).toBe(false);
		expect(shouldShowConversationStarting(messages, "Next prompt", "running")).toBe(true);
	});

	it("advances the preparation copy from actual session and turn state", () => {
		expect(conversationStartingPhase(null, null)).toBe("creating");
		expect(conversationStartingPhase("session-1", null)).toBe("sending");
		expect(conversationStartingPhase("session-1", "running")).toBe("waiting");
	});
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
