import { describe, expect, it } from "vitest";
import {
	ASSISTANT_ACTIVITY_AUTO_COLLAPSE_DELAY_MS,
	ASSISTANT_ACTIVITY_PAGE_SIZE,
	isActivityAnnotation,
	selectAssistantActivity,
	summarizeAssistantActivity,
	type AssistantActivityAnnotation,
} from "./assistant-activity-disclosure";

const annotations: AssistantActivityAnnotation[] = [
	{ type: "commentary", id: "progress", content: "Inspecting the project" },
	{
		type: "tool-call",
		id: "failed",
		action: "Test",
		content: "A test failed",
		status: { type: "failed" },
	},
	{ type: "reasoning", id: "reason", content: "Reviewing the result" },
	{
		type: "tool-call",
		id: "live",
		action: "Read",
		content: "",
		streaming: true,
	},
];
describe("assistant activity presentation", () => {
	it("preserves original order and sequence numbers when filtering failures and updates", () => {
		expect(
			selectAssistantActivity(annotations, "all").entries.map(
				(item) => item.annotation.id,
			),
		).toEqual(["progress", "failed", "reason", "live"]);
		expect(
			selectAssistantActivity(annotations, "failures").entries.map(
				(item) => item.index,
			),
		).toEqual([1]);
		expect(
			selectAssistantActivity(annotations, "updates").entries.map(
				(item) => item.index,
			),
		).toEqual([0, 2]);
		expect(
			selectAssistantActivity(annotations, "tools").entries.map(
				(item) => item.index,
			),
		).toEqual([1, 3]);
	});
	it("bounds initial history and reveals earlier entries without changing order", () => {
		const history = Array.from(
			{ length: 1000 },
			(_, index): AssistantActivityAnnotation => ({
				type: "tool-call",
				id: String(index),
				action: "Read",
				content: "result",
			}),
		);
		const initial = selectAssistantActivity(history, "all");
		expect(initial.entries).toHaveLength(ASSISTANT_ACTIVITY_PAGE_SIZE);
		expect(initial.hidden).toBe(980);
		expect(initial.entries[0]?.index).toBe(980);
		const expanded = selectAssistantActivity(history, "all", 40);
		expect(expanded.entries[0]?.index).toBe(960);
		expect(expanded.entries.slice(-20)).toEqual(initial.entries);
	});
	it("shows live activity and counts failures without treating the whole turn as failed", () => {
		const summary = summarizeAssistantActivity(annotations, true);
		expect(summary.state).toBe("running");
		expect(summary.latest?.id).toBe("live");
		expect(summary.failures).toBe(1);
		expect(summary.tools).toBe(2);
		expect(summary.updates).toBe(2);
		expect(
			summarizeAssistantActivity(
				[
					...annotations,
					{ type: "commentary", id: "newest", content: "A newer event" },
				],
				true,
			).latest?.id,
		).toBe("newest");
	});
	it("uses explicit completion or interruption over stale annotation streaming flags", () => {
		expect(summarizeAssistantActivity(annotations, false).live).toBe(false);
		expect(summarizeAssistantActivity(annotations, true, true).state).toBe(
			"interrupted",
		);
		expect(summarizeAssistantActivity(annotations, true, true).live).toBe(
			false,
		);
		expect(
			summarizeAssistantActivity(annotations, false, false, true).state,
		).toBe("history");
	});
	it("distinguishes waiting for user input from agent work and stays live between events", () => {
		expect(
			summarizeAssistantActivity(annotations, true, false, true).state,
		).toBe("waiting");
		expect(
			summarizeAssistantActivity(annotations.slice(0, 3), true).state,
		).toBe("running");
		expect(
			summarizeAssistantActivity(annotations.slice(0, 3), true).latest?.id,
		).toBe("reason");
	});
	it("keeps approvals, user questions, and native agent controls outside tool history", () => {
		expect(
			isActivityAnnotation({
				type: "native-subagent",
				id: "agent",
				status: "running",
			}),
		).toBe(false);
		expect(
			isActivityAnnotation({
				type: "approval",
				id: "approval",
				toolName: "shell",
				streaming: true,
			}),
		).toBe(false);
		expect(
			isActivityAnnotation({
				type: "user-input",
				id: "input",
				questions: [],
				answers: [],
				streaming: true,
			}),
		).toBe(false);
	});
	it("uses a short grace period before automatic collapse", () => {
		expect(ASSISTANT_ACTIVITY_AUTO_COLLAPSE_DELAY_MS).toBeGreaterThanOrEqual(
			300,
		);
		expect(ASSISTANT_ACTIVITY_AUTO_COLLAPSE_DELAY_MS).toBeLessThanOrEqual(500);
	});
});
