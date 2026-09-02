import { describe, expect, it } from "vitest";
import type { WorkspaceMessage } from "./thread-projection";
import {
	buildSafeContinuationPrompt,
	latestConversationActivitySignature,
	precedingUserPrompt,
	precedingUserTurn,
} from "./conversation-recovery";

function message(overrides: Partial<WorkspaceMessage>): WorkspaceMessage {
	return {
		id: "message-1",
		role: "assistant",
		content: "",
		label: "Assistant",
		...overrides,
	};
}

describe("conversation recovery", () => {
	it("builds a continuation that checks partial work before editing", () => {
		const prompt = buildSafeContinuationPrompt({
			originalPrompt: "Implement authentication",
			preamble:
				"Continue a tarefa. Antes de alterar, verifique o que já foi concluído.",
			originalLabel: "Pedido original",
		});

		expect(prompt).toContain("verifique o que já foi concluído");
		expect(prompt).toContain("Pedido original:\nImplement authentication");
	});

	it("finds the user request preceding an interrupted assistant turn", () => {
		const messages = [
			message({ id: "user-1", role: "user", content: "First request" }),
			message({ id: "assistant-1", content: "Done" }),
			message({ id: "user-2", role: "user", content: "Second request" }),
			message({ id: "assistant-2", status: { type: "incomplete" } }),
		];

		expect(precedingUserPrompt(messages, 3)).toBe("Second request");
	});

	it("detects streaming updates without requiring a new message", () => {
		const before = message({ id: "assistant-1", content: "Working", streaming: true });
		const after = message({ id: "assistant-1", content: "Workings", streaming: true });

		expect(latestConversationActivitySignature([before])).not.toBe(
			latestConversationActivitySignature([after]),
		);
	});
});

describe("precedingUserTurn", () => {
	it("returns the nearest earlier user turn with its id", () => {
		const messages = [
			{ id: "u1", role: "user" as const, turnId: "turn-1", label: "User", content: "first" },
			{ id: "a1", role: "assistant" as const, label: "Assistant", content: "reply" },
			{ id: "u2", role: "user" as const, label: "User", content: "second" },
			{ id: "a2", role: "assistant" as const, label: "Assistant", content: "…" },
		];
		expect(precedingUserTurn(messages, 3)).toEqual({ prompt: "second", turnId: null });
		expect(precedingUserTurn(messages, 1)).toEqual({ prompt: "first", turnId: "turn-1" });
		expect(precedingUserTurn(messages, 0)).toBeNull();
	});
});
