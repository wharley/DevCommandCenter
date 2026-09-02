import type {
	ObjectiveTransition,
	SessionObjective,
	SessionObjectiveDraft,
} from "@dcc/contracts";

export const OBJECTIVE_TEXT_MAX_CHARS = 2_000;
export const OBJECTIVE_DEFAULT_MAX_CONSECUTIVE_FAILURES = 3;
export const OBJECTIVE_MAX_CONSECUTIVE_FAILURES = 20;
export const OBJECTIVE_MAX_TURNS = 10_000;

/** Person-editable form state; numbers stay strings until submit. */
export type ObjectiveFormDraft = {
	intent: string;
	doneWhen: string;
	maxConsecutiveFailures: string;
	maxTurns: string;
};

export const EMPTY_OBJECTIVE_FORM: ObjectiveFormDraft = {
	intent: "",
	doneWhen: "",
	maxConsecutiveFailures: String(OBJECTIVE_DEFAULT_MAX_CONSECUTIVE_FAILURES),
	maxTurns: "",
};

export function objectiveFormFromRecord(
	objective: SessionObjective | null | undefined,
): ObjectiveFormDraft {
	if (!objective) return EMPTY_OBJECTIVE_FORM;
	return {
		intent: objective.intent,
		doneWhen: objective.doneWhen,
		maxConsecutiveFailures: String(objective.maxConsecutiveFailures),
		maxTurns: objective.maxTurns == null ? "" : String(objective.maxTurns),
	};
}

export type ObjectiveFormError =
	| "intent_required"
	| "text_too_long"
	| "failures_out_of_range"
	| "turns_out_of_range";

function parseBoundedInteger(value: string, min: number, max: number) {
	const trimmed = value.trim();
	if (trimmed.length === 0) return { value: null, valid: true };
	if (!/^\d+$/.test(trimmed)) return { value: null, valid: false };
	const parsed = Number(trimmed);
	return { value: parsed, valid: parsed >= min && parsed <= max };
}

/** Mirrors the backend validation so the form fails fast with a clear reason. */
export function objectiveDraftFromForm(
	form: ObjectiveFormDraft,
): { draft: SessionObjectiveDraft; error: null } | { draft: null; error: ObjectiveFormError } {
	const intent = form.intent.trim();
	const doneWhen = form.doneWhen.trim();
	if (intent.length === 0) return { draft: null, error: "intent_required" };
	if (
		[...intent].length > OBJECTIVE_TEXT_MAX_CHARS ||
		[...doneWhen].length > OBJECTIVE_TEXT_MAX_CHARS
	) {
		return { draft: null, error: "text_too_long" };
	}
	const failures = parseBoundedInteger(
		form.maxConsecutiveFailures,
		1,
		OBJECTIVE_MAX_CONSECUTIVE_FAILURES,
	);
	if (!failures.valid) return { draft: null, error: "failures_out_of_range" };
	const turns = parseBoundedInteger(form.maxTurns, 1, OBJECTIVE_MAX_TURNS);
	if (!turns.valid) return { draft: null, error: "turns_out_of_range" };
	return {
		draft: {
			intent,
			doneWhen,
			maxConsecutiveFailures: failures.value,
			maxTurns: turns.value,
		},
		error: null,
	};
}

/** Which transitions make sense for the current status. */
export function availableObjectiveTransitions(
	objective: SessionObjective,
): ObjectiveTransition[] {
	switch (objective.status) {
		case "active":
			return ["pause", "complete"];
		case "paused":
			return ["resume", "complete"];
		case "done":
			return ["resume"];
		default:
			return [];
	}
}

export type ObjectiveSummary = {
	status: SessionObjective["status"];
	pauseReason: SessionObjective["pauseReason"];
	turnsLabel: string;
	failuresLabel: string;
	/** Explicit retries counted on this objective; 0 when none. */
	retries: number;
	/** True when automatic follow-ups are blocked until the person acts. */
	blocksAutomaticDispatch: boolean;
};

export function summarizeObjective(objective: SessionObjective): ObjectiveSummary {
	return {
		status: objective.status,
		pauseReason: objective.pauseReason ?? null,
		turnsLabel:
			objective.maxTurns == null
				? String(objective.turnsUsed)
				: `${objective.turnsUsed}/${objective.maxTurns}`,
		failuresLabel: `${objective.consecutiveFailures}/${objective.maxConsecutiveFailures}`,
		retries: objective.retries ?? 0,
		blocksAutomaticDispatch: objective.status !== "active",
	};
}
