import { describe, expect, it } from "vitest";
import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";
import { shouldAutoOpenAssistantActivity } from "./assistant-activity-disclosure";

const annotation = (value: object) => value as WorkspaceMessageAnnotation;

describe("assistant activity disclosure", () => {
	it("stays collapsed by default for live, failed and settled activity", () => {
		expect(
			shouldAutoOpenAssistantActivity([
				annotation({ type: "reasoning", streaming: true }),
			]),
		).toBe(false);
		expect(
			shouldAutoOpenAssistantActivity([
				annotation({ type: "tool-call", status: { type: "failed" } }),
			]),
		).toBe(false);
		expect(
			shouldAutoOpenAssistantActivity([
				annotation({ type: "tool-call", status: { type: "completed" } }),
			]),
		).toBe(false);
	});
});
