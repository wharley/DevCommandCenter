import type { SessionGoal } from "@dcc/contracts";

/** How the goal strip reads a goal: which title, what usage, which action. */
export type GoalPresentation = {
	titleKey:
		| "goal.titleWorking"
		| "goal.titleActive"
		| "goal.titlePaused"
		| "goal.titleBlocked"
		| "goal.titleUsageLimited"
		| "goal.titleBudgetLimited"
		| "goal.titleComplete";
	tone: "active" | "attention" | "done";
	checks: number;
	tokensUsed: number | null;
	tokenBudget: number | null;
	minutes: number | null;
	/** Why the latest check said it is not done yet; never on a finished goal. */
	lastCheck: string | null;
	/** What the person can do while the agent is not working on it. */
	action: "clear" | "resume_and_clear" | "dismiss";
};

const STOPPED_TITLES = {
	paused: "goal.titlePaused",
	blocked: "goal.titleBlocked",
	usage_limited: "goal.titleUsageLimited",
	budget_limited: "goal.titleBudgetLimited",
} as const;

export function presentGoal(goal: SessionGoal, working: boolean): GoalPresentation {
	const usage = {
		checks: goal.checks ?? 0,
		tokensUsed: goal.tokensUsed && goal.tokensUsed > 0 ? goal.tokensUsed : null,
		tokenBudget: goal.tokenBudget ?? null,
		minutes:
			goal.timeUsedSeconds != null && goal.timeUsedSeconds >= 60
				? Math.round(goal.timeUsedSeconds / 60)
				: null,
	};
	if (goal.status === "complete") {
		return { ...usage, titleKey: "goal.titleComplete", tone: "done", lastCheck: null, action: "dismiss" };
	}
	const lastCheck = goal.lastCheck?.trim() || null;
	if (goal.status === "active") {
		return {
			...usage,
			titleKey: working ? "goal.titleWorking" : "goal.titleActive",
			tone: "active",
			lastCheck,
			action: "clear",
		};
	}
	return {
		...usage,
		titleKey: STOPPED_TITLES[goal.status],
		tone: "attention",
		lastCheck,
		action: "resume_and_clear",
	};
}

/** Compact token count: 950, 12k, 1.2M. */
export function formatGoalTokens(tokens: number): string {
	if (tokens < 1_000) return `${tokens}`;
	if (tokens < 1_000_000) {
		const thousands = tokens / 1_000;
		return `${thousands < 10 ? thousands.toFixed(1).replace(/\.0$/, "") : Math.round(thousands)}k`;
	}
	const millions = tokens / 1_000_000;
	return `${millions < 10 ? millions.toFixed(1).replace(/\.0$/, "") : Math.round(millions)}M`;
}

/**
 * The turn that continues a stopped goal. Codex and Grok resume their goal
 * natively; Claude has no resume, any message continues it.
 */
export function goalResumePrompt(providerId: string | null | undefined): string {
	return providerId === "codex" || providerId === "grok" ? "/goal resume" : "Continue de onde parou.";
}

/**
 * The turn that drops the goal. Cursor has no `/goal clear`: its agent ends a
 * goal itself (`UpdateGoal`) when the person asks it to stop.
 */
export function goalClearPrompt(providerId: string | null | undefined): string {
	return providerId === "cursor"
		? "Encerre o objetivo agora: marque-o como concluído sem fazer mais nada."
		: "/goal clear";
}
