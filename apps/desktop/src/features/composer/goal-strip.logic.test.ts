import { describe, expect, it } from "vitest";
import type { SessionGoal } from "@dcc/contracts";
import { formatGoalTokens, goalClearPrompt, goalResumePrompt, presentGoal } from "./goal-strip.logic";

const goal = (overrides: Partial<SessionGoal> = {}): SessionGoal => ({
	objective: "tests pass",
	status: "active",
	checks: 0,
	lastCheck: null,
	tokensUsed: null,
	tokenBudget: null,
	timeUsedSeconds: null,
	completesWithTurn: true,
	updatedAt: "2026-10-18T10:00:00Z",
	...overrides,
});

describe("presentGoal", () => {
	it("says the agent is working on an active goal only while a turn runs", () => {
		expect(presentGoal(goal(), true).titleKey).toBe("goal.titleWorking");
		expect(presentGoal(goal(), false).titleKey).toBe("goal.titleActive");
		expect(presentGoal(goal(), false).action).toBe("clear");
	});

	it("keeps the latest reason while the goal is open and drops it once complete", () => {
		const open = presentGoal(goal({ checks: 2, lastCheck: " count is 2 " }), true);
		expect(open.checks).toBe(2);
		expect(open.lastCheck).toBe("count is 2");
		const done = presentGoal(goal({ status: "complete", checks: 2, lastCheck: "count is 2" }), false);
		expect(done.titleKey).toBe("goal.titleComplete");
		expect(done.lastCheck).toBeNull();
		expect(done.action).toBe("dismiss");
		expect(done.tone).toBe("done");
	});

	it("offers to continue a stopped goal", () => {
		for (const status of ["paused", "blocked", "usage_limited", "budget_limited"] as const) {
			const presented = presentGoal(goal({ status }), false);
			expect(presented.action).toBe("resume_and_clear");
			expect(presented.tone).toBe("attention");
		}
		expect(presentGoal(goal({ status: "budget_limited" }), false).titleKey).toBe(
			"goal.titleBudgetLimited",
		);
	});

	it("shows provider usage only when it says something", () => {
		const quiet = presentGoal(goal({ tokensUsed: 0, timeUsedSeconds: 30 }), false);
		expect(quiet.tokensUsed).toBeNull();
		expect(quiet.minutes).toBeNull();
		const busy = presentGoal(goal({ tokensUsed: 12_400, tokenBudget: 50_000, timeUsedSeconds: 600 }), false);
		expect(busy.tokensUsed).toBe(12_400);
		expect(busy.tokenBudget).toBe(50_000);
		expect(busy.minutes).toBe(10);
	});
});

describe("formatGoalTokens", () => {
	it("keeps counts short", () => {
		expect(formatGoalTokens(950)).toBe("950");
		expect(formatGoalTokens(1_000)).toBe("1k");
		expect(formatGoalTokens(1_250)).toBe("1.3k");
		expect(formatGoalTokens(12_400)).toBe("12k");
		expect(formatGoalTokens(1_200_000)).toBe("1.2M");
	});
});

describe("goalResumePrompt", () => {
	it("resumes Codex and Grok natively and continues Claude with a message", () => {
		expect(goalResumePrompt("codex")).toBe("/goal resume");
		expect(goalResumePrompt("grok")).toBe("/goal resume");
		expect(goalResumePrompt("claude_code")).toBe("Continue de onde parou.");
	});
});

describe("goalClearPrompt", () => {
	it("uses the provider command, or asks Cursor's agent to end the goal", () => {
		expect(goalClearPrompt("claude_code")).toBe("/goal clear");
		expect(goalClearPrompt("codex")).toBe("/goal clear");
		expect(goalClearPrompt("grok")).toBe("/goal clear");
		expect(goalClearPrompt("cursor")).toMatch(/^Encerre o objetivo/);
	});
});
