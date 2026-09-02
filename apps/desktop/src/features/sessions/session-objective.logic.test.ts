import { describe, expect, it } from "vitest";
import type { SessionObjective } from "@dcc/contracts";
import {
	EMPTY_OBJECTIVE_FORM,
	OBJECTIVE_TEXT_MAX_CHARS,
	availableObjectiveTransitions,
	objectiveDraftFromForm,
	objectiveFormFromRecord,
	summarizeObjective,
} from "./session-objective.logic";

function objective(overrides: Partial<SessionObjective> = {}): SessionObjective {
	return {
		sessionId: "s1",
		intent: "ship it",
		doneWhen: "tests pass",
		status: "active",
		pauseReason: null,
		maxConsecutiveFailures: 3,
		maxTurns: 20,
		turnsUsed: 4,
		consecutiveFailures: 1,
		lastCountedTurnId: "t4",
		generation: 7,
		updatedAt: "2026-09-02T12:00:00Z",
		...overrides,
	};
}

describe("session objective logic", () => {
	it("round-trips a record into the form and back into a draft", () => {
		const form = objectiveFormFromRecord(objective());
		expect(form).toEqual({
			intent: "ship it",
			doneWhen: "tests pass",
			maxConsecutiveFailures: "3",
			maxTurns: "20",
		});
		expect(objectiveDraftFromForm(form)).toEqual({
			draft: { intent: "ship it", doneWhen: "tests pass", maxConsecutiveFailures: 3, maxTurns: 20 },
			error: null,
		});
		expect(objectiveFormFromRecord(null)).toBe(EMPTY_OBJECTIVE_FORM);
	});

	it("rejects what the backend would reject, before any request", () => {
		expect(objectiveDraftFromForm({ ...EMPTY_OBJECTIVE_FORM, intent: "  " }).error).toBe(
			"intent_required",
		);
		expect(
			objectiveDraftFromForm({
				...EMPTY_OBJECTIVE_FORM,
				intent: "x".repeat(OBJECTIVE_TEXT_MAX_CHARS + 1),
			}).error,
		).toBe("text_too_long");
		expect(
			objectiveDraftFromForm({ ...EMPTY_OBJECTIVE_FORM, intent: "x", maxConsecutiveFailures: "0" })
				.error,
		).toBe("failures_out_of_range");
		expect(
			objectiveDraftFromForm({ ...EMPTY_OBJECTIVE_FORM, intent: "x", maxTurns: "abc" }).error,
		).toBe("turns_out_of_range");
		const unlimited = objectiveDraftFromForm({
			...EMPTY_OBJECTIVE_FORM,
			intent: "x",
			maxConsecutiveFailures: "",
			maxTurns: "",
		});
		expect(unlimited.draft).toEqual({
			intent: "x",
			doneWhen: "",
			maxConsecutiveFailures: null,
			maxTurns: null,
		});
	});

	it("offers only meaningful transitions and explains dispatch blocking", () => {
		expect(availableObjectiveTransitions(objective())).toEqual(["pause", "complete"]);
		expect(availableObjectiveTransitions(objective({ status: "paused" }))).toEqual([
			"resume",
			"complete",
		]);
		expect(availableObjectiveTransitions(objective({ status: "done" }))).toEqual(["resume"]);
		expect(summarizeObjective(objective())).toEqual({
			status: "active",
			pauseReason: null,
			turnsLabel: "4/20",
			failuresLabel: "1/3",
			blocksAutomaticDispatch: false,
		});
		expect(
			summarizeObjective(
				objective({ status: "paused", pauseReason: "turn_budget", maxTurns: null }),
			),
		).toMatchObject({ turnsLabel: "4", blocksAutomaticDispatch: true, pauseReason: "turn_budget" });
	});
});
