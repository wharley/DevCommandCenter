import { describe, expect, it } from "vitest";
import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";
import { shouldAutoOpenAssistantActivity } from "./assistant-activity-disclosure";

const annotation = (value: object) => value as WorkspaceMessageAnnotation;

describe("assistant activity disclosure", () => {
	it("auto-opens while live or failed, but not for settled successful activity", () => {
		expect(
			shouldAutoOpenAssistantActivity([
				annotation({ type: "reasoning", streaming: true }),
			]),
		).toBe(true);
		expect(
			shouldAutoOpenAssistantActivity([
				annotation({ type: "tool-call", status: { type: "failed" } }),
			]),
		).toBe(true);
		expect(
			shouldAutoOpenAssistantActivity([
				annotation({ type: "tool-call", status: { type: "completed" } }),
			]),
		).toBe(false);
	});
});
