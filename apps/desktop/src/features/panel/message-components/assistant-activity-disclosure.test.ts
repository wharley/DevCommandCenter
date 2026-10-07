import { describe, expect, it } from "vitest";
import { isActivityAnnotation } from "./assistant-activity-disclosure";

describe("assistant activity annotations", () => {
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
});
