import { describe, expect, it } from "vitest";
import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";
import { shouldAutoOpenAssistantActivity } from "./assistant-activity-disclosure";

const annotation = (value: object) => value as WorkspaceMessageAnnotation;

describe("assistant activity disclosure", () => {
	it("opens for live and failed activity but not settled activity", () => {
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

	it("does not treat a native subagent as parent activity", () => {
		expect(
			shouldAutoOpenAssistantActivity([
				annotation({ type: "native-subagent", status: "running" }),
			]),
		).toBe(false);
		expect(
			shouldAutoOpenAssistantActivity([
				annotation({ type: "native-subagent", status: "failed" }),
			]),
		).toBe(false);
	});
});
