import { describe, expect, it } from "vitest";
import type { McpRuntimeStatus } from "@dcc/contracts";
import type { WorkspaceMessage } from "./thread-projection";
import {
	appendStartupPrompt,
	conversationStartupStage,
	mcpStartupProgress,
	shouldShowConversationStarting,
} from "./conversation-starting.logic";

function message(
	id: string,
	role: WorkspaceMessage["role"],
	turnId?: string,
): WorkspaceMessage {
	return { id, role, label: role, content: id, turnId };
}

function mcp(state: McpRuntimeStatus["state"], providerId = "codex"): McpRuntimeStatus {
	return {
		definitionId: `mcp-${state}`,
		providerId,
		providerVersion: "1",
		sessionId: "session-1",
		state,
		checkedAt: "2026-10-16T12:00:00Z",
		boundedError: null,
	} as McpRuntimeStatus;
}

describe("appendStartupPrompt", () => {
	it("shows the first prompt before a session exists", () => {
		expect(appendStartupPrompt([], "  First prompt ")).toEqual([
			{ id: "pending-user", role: "user", label: "User", content: "First prompt" },
		]);
	});

	it("does not duplicate a prompt the projection already shows", () => {
		const pending = { ...message("pending-user-s", "user"), content: "Next" };
		expect(appendStartupPrompt([pending], "Next")).toEqual([pending]);
		const accepted = { ...message("user-s-t", "user", "t"), content: "Next" };
		expect(appendStartupPrompt([accepted], "Next")).toEqual([accepted]);
	});

	it("adds a repeated prompt once the earlier one was answered", () => {
		const messages = [{ ...message("u", "user", "t"), content: "Again" }, message("a", "assistant", "t")];
		expect(appendStartupPrompt(messages, "Again")).toHaveLength(3);
		expect(appendStartupPrompt(messages, null)).toBe(messages);
	});
});

describe("conversationStartupStage", () => {
	const base = { isModelRouting: false, sessionId: "s", lastTurnState: null, mcp: null };
	it("follows routing, session creation, acceptance and MCP startup", () => {
		expect(conversationStartupStage({ ...base, isModelRouting: true })).toBe("routing");
		expect(conversationStartupStage({ ...base, sessionId: null })).toBe("creating");
		expect(conversationStartupStage(base)).toBe("sending");
		expect(conversationStartupStage({ ...base, lastTurnState: "running" })).toBe("waiting");
		expect(
			conversationStartupStage({ ...base, lastTurnState: "running", mcp: { ready: 1, total: 3 } }),
		).toBe("connecting");
	});
});

describe("mcpStartupProgress", () => {
	it("counts servers still coming up for the session's provider", () => {
		expect(
			mcpStartupProgress(
				[mcp("connected"), mcp("attachingProvider"), mcp("probingServer"), mcp("failed"), mcp("disabled"), mcp("attachingProvider", "claude_code")],
				"codex",
			),
		).toEqual({ ready: 2, total: 4 });
	});

	it("says nothing once every server settled", () => {
		expect(mcpStartupProgress([mcp("connected"), mcp("failed")], "codex")).toBeNull();
		expect(mcpStartupProgress([], "codex")).toBeNull();
	});
});

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
