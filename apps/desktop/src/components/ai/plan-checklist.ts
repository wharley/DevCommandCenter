export type PlanStepStatus = "completed" | "in_progress" | "pending";

export type PlanStep = {
	text: string;
	status: PlanStepStatus;
};

export type PlanChecklist = {
	steps: PlanStep[];
	completed: number;
	/** The step being worked on, or the next one when none is marked. */
	current: PlanStep | null;
};

function stepStatus(value: unknown): PlanStepStatus {
	if (value === true || value === "completed" || value === "done") return "completed";
	if (value === "in_progress" || value === "inProgress" || value === "active") {
		return "in_progress";
	}
	return "pending";
}

function readStep(raw: unknown): PlanStep | null {
	if (!raw || typeof raw !== "object") return null;
	const step = raw as Record<string, unknown>;
	const status = stepStatus(step.status ?? step.completed);
	// Claude shows the present tense ("Running tests") while a step runs.
	const text =
		(status === "in_progress" && typeof step.activeForm === "string" && step.activeForm) ||
		[step.content, step.step, step.text].find(
			(value): value is string => typeof value === "string" && value.trim().length > 0,
		);
	return text ? { text: text.trim(), status } : null;
}

/**
 * The agent's own step list, from the input of its plan tool: Claude's
 * TodoWrite (`todos`), Codex's plan update (`plan`) or todo list (`items`).
 * Anything else is not a checklist.
 */
export function parsePlanChecklist(input: string | null | undefined): PlanChecklist | null {
	if (!input) return null;
	let parsed: unknown;
	try {
		parsed = JSON.parse(input);
	} catch {
		return null;
	}
	if (!parsed || typeof parsed !== "object") return null;
	const record = parsed as Record<string, unknown>;
	const list = [record.todos, record.plan, record.items].find(Array.isArray) as
		| unknown[]
		| undefined;
	if (!list) return null;
	const steps = list.map(readStep).filter((step): step is PlanStep => step !== null);
	if (steps.length === 0) return null;
	return {
		steps,
		completed: steps.filter((step) => step.status === "completed").length,
		current:
			steps.find((step) => step.status === "in_progress") ??
			steps.find((step) => step.status === "pending") ??
			null,
	};
}
