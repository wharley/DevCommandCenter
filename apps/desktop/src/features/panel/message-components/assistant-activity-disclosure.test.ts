import { describe, expect, it } from "vitest";
import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";
import {
	ASSISTANT_ACTIVITY_AUTO_COLLAPSE_DELAY_MS,
	partitionAssistantActivity,
	shouldAutoOpenAssistantActivity,
} from "./assistant-activity-disclosure";

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

	it("stays open while the parent turn streams between activity events", () => {
		expect(
			shouldAutoOpenAssistantActivity(
				[annotation({ type: "tool-call", status: { type: "completed" } })],
				true,
			),
		).toBe(true);
	});

	it("separates the current and failed activity from compactable history", () => {
		expect(
			partitionAssistantActivity([
				annotation({ type: "commentary", id: "progress-1" }),
				annotation({ type: "tool-call", id: "tool-1" }),
				annotation({ type: "reasoning", id: "reasoning-1", streaming: true }),
				annotation({
					type: "tool-call",
					id: "tool-2",
					status: { type: "failed" },
				}),
			]),
		).toEqual({
			historyIndexes: [0, 1],
			prominentIndexes: [2, 3],
		});
	});

	it("uses a short grace period before automatically collapsing", () => {
		expect(ASSISTANT_ACTIVITY_AUTO_COLLAPSE_DELAY_MS).toBeGreaterThanOrEqual(300);
		expect(ASSISTANT_ACTIVITY_AUTO_COLLAPSE_DELAY_MS).toBeLessThanOrEqual(500);
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
