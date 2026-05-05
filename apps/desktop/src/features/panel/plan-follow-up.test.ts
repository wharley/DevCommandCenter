import { describe, expect, it } from "vitest";
import { parsePlanContent } from "./plan-content";
import { derivePlanFollowUpState } from "./plan-follow-up";
import type { WorkspaceMessage } from "./thread-projection";

const PLAN_MARKDOWN = `# Mission Plan

## Summary
Ship the feature.

## Steps
- [ ] Update the UI
- [ ] Add tests
`;

function makePlanMessage(
	overrides: Partial<WorkspaceMessage> = {},
): WorkspaceMessage {
	return {
		id: "assistant-plan",
		role: "assistant",
		label: "Assistant",
		content: PLAN_MARKDOWN,
		plan: parsePlanContent(PLAN_MARKDOWN),
		...overrides,
	};
}

describe("derivePlanFollowUpState", () => {
	it("returns no plan follow-up state when the thread has no plan", () => {
		const state = derivePlanFollowUpState([
			{
				id: "user-1",
				role: "user",
				label: "User",
				content: "hello",
			},
		]);

		expect(state.latestPlanMessage).toBeNull();
		expect(state.activePlanMessage).toBeNull();
		expect(state.showPlanFollowUpPrompt).toBe(false);
	});

	it("keeps the banner visible when the latest conversational message is a completed plan", () => {
		const planMessage = makePlanMessage();
		const state = derivePlanFollowUpState([
			planMessage,
			{
				id: "system-1",
				role: "system",
				label: "session.completed",
				content: "s-1",
			},
		]);

		expect(state.latestPlanMessage).toBe(planMessage);
		expect(state.activePlanMessage).toBe(planMessage);
		expect(state.showPlanFollowUpPrompt).toBe(true);
	});

	it("hides the banner after a later user prompt changes the thread context", () => {
		const planMessage = makePlanMessage();
		const state = derivePlanFollowUpState([
			planMessage,
			{
				id: "user-2",
				role: "user",
				label: "User",
				content: "implement it",
			},
		]);

		expect(state.latestPlanMessage).toBe(planMessage);
		expect(state.activePlanMessage).toBeNull();
		expect(state.showPlanFollowUpPrompt).toBe(false);
	});

	it("hides the banner after a later assistant reply changes the thread context", () => {
		const planMessage = makePlanMessage();
		const state = derivePlanFollowUpState([
			planMessage,
			{
				id: "assistant-2",
				role: "assistant",
				label: "Assistant",
				content: "Implementation started",
			},
		]);

		expect(state.latestPlanMessage).toBe(planMessage);
		expect(state.activePlanMessage).toBeNull();
		expect(state.showPlanFollowUpPrompt).toBe(false);
	});

	it("does not expose follow-up actions while the plan is still streaming", () => {
		const planMessage = makePlanMessage({ streaming: true });
		const state = derivePlanFollowUpState([planMessage]);

		expect(state.latestPlanMessage).toBe(planMessage);
		expect(state.activePlanMessage).toBe(planMessage);
		expect(state.showPlanFollowUpPrompt).toBe(false);
	});
});
