import { describe, expect, it } from "vitest";
import {
	isDelegateTaskTool,
	parseAgentInitiatedDelegationRequest,
} from "./agent-delegation-request";

describe("agent delegation request", () => {
	it("parses delegate_task JSON payloads", () => {
		const request = parseAgentInitiatedDelegationRequest({
			command: JSON.stringify({
				instruction: "Review the auth diff.",
				mode: "review",
				contextPolicy: "review_current_diff",
				targetProviderId: "codex",
				targetModelId: "gpt-5",
			}),
		});

		expect(request).toEqual({
			instruction: "Review the auth diff.",
			mode: "review",
			contextPolicy: { type: "review_current_diff" },
			targetProviderId: "codex",
			targetModelId: "gpt-5",
		});
	});

	it("falls back to raw command text as the instruction", () => {
		const request = parseAgentInitiatedDelegationRequest({
			command: "Explain the failing test.",
		});

		expect(request?.instruction).toBe("Explain the failing test.");
		expect(request?.mode).toBe("review");
		expect(request?.targetProviderId).toBeNull();
	});

	it("matches delegate_task case-insensitively", () => {
		expect(isDelegateTaskTool(" Delegate_Task ")).toBe(true);
	});
});
