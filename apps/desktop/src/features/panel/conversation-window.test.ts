import { describe, expect, it } from "vitest";
import type { WorkspaceMessage } from "./thread-projection";
import { conversationWindowStart } from "./conversation-window";

function message(id: string, role: WorkspaceMessage["role"]): WorkspaceMessage {
	return { id, role, content: id, label: role };
}

describe("conversation window", () => {
	it("keeps short conversations fully mounted", () => {
		const messages = [message("u1", "user"), message("a1", "assistant")];
		expect(conversationWindowStart(messages, 80)).toBe(0);
	});

	it("aligns the window to the beginning of a user turn", () => {
		const messages = [
			message("u1", "user"),
			message("a1", "assistant"),
			message("s1", "system"),
			message("u2", "user"),
			message("a2", "assistant"),
			message("s2", "system"),
		];
		expect(conversationWindowStart(messages, 2)).toBe(3);
	});

	it("reveals the full conversation as the limit grows", () => {
		const messages = Array.from({ length: 10 }, (_, index) =>
			message(`m${index}`, index % 2 === 0 ? "user" : "assistant"),
		);
		expect(conversationWindowStart(messages, 4)).toBe(6);
		expect(conversationWindowStart(messages, 20)).toBe(0);
	});
});
