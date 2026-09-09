import { describe, expect, it } from "vitest";
import { visibleSessionPendingPrompt } from "./pending-prompt";

const starting = {
	prompt: "Review authentication",
	pendingSessionId: null,
	startingWorkspaceId: "workspace-1",
	workspaceId: "workspace-1",
	sessionId: null,
};

describe("visibleSessionPendingPrompt", () => {
	it("keeps preparation visible when the catalog discovers the session before startThread resolves", () => {
		expect(visibleSessionPendingPrompt(starting)).toBe(starting.prompt);
		expect(visibleSessionPendingPrompt({ ...starting, sessionId: "session-1" })).toBe(starting.prompt);
		expect(visibleSessionPendingPrompt({ ...starting, sessionId: "session-1", pendingSessionId: "session-1" })).toBe(starting.prompt);
	});

	it("does not expose a starting prompt in another workspace", () => {
		expect(visibleSessionPendingPrompt({ ...starting, workspaceId: "workspace-2" })).toBeNull();
		expect(visibleSessionPendingPrompt({ ...starting, workspaceId: null })).toBeNull();
	});

	it("restricts an anchored prompt to its session, including follow-up turns", () => {
		const anchored = { ...starting, startingWorkspaceId: null, pendingSessionId: "session-1" };
		expect(visibleSessionPendingPrompt({ ...anchored, sessionId: "session-1" })).toBe(starting.prompt);
		expect(visibleSessionPendingPrompt({ ...anchored, sessionId: "session-2" })).toBeNull();
		expect(visibleSessionPendingPrompt(anchored)).toBeNull();
	});

	it("clears preparation after acceptance or rejection clears the prompt", () => {
		expect(visibleSessionPendingPrompt({ ...starting, prompt: null, sessionId: "session-1" })).toBeNull();
	});
});
