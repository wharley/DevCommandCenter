/**
 * The `[DCC] …` turns the backend writes into a parent conversation when a
 * delegation finishes or its edits are applied/discarded (see
 * `delegation_result_turn` and `delegation_review_outcome_turn` in
 * `delegation_runtime.rs`). They are addressed to the agent, so the thread
 * shows them as a one-line system event instead of a message the person wrote.
 */
export type DelegationHandBackOutcome = "finished" | "failed" | "applied" | "discarded";

export type DelegationHandBack = {
	delegationId: string;
	outcome: DelegationHandBackOutcome;
};

const RESULT_PATTERN = /^\[DCC\] Delegated \w+ task (finished|failed) — .*\(task ([^)\s]+)\)\./;
const OUTCOME_PATTERN =
	/^\[DCC\] The human (APPLIED|DISCARDED) the edits from your delegated \w+ task — .*\(task ([^)\s]+)\)\./;

export function parseDelegationHandBack(content: string): DelegationHandBack | null {
	const firstLine = content.trimStart().split("\n", 1)[0] ?? "";
	const result = RESULT_PATTERN.exec(firstLine);
	if (result?.[1] && result[2]) {
		return {
			delegationId: result[2],
			outcome: result[1] === "failed" ? "failed" : "finished",
		};
	}
	const outcome = OUTCOME_PATTERN.exec(firstLine);
	if (outcome?.[1] && outcome[2]) {
		return {
			delegationId: outcome[2],
			outcome: outcome[1] === "APPLIED" ? "applied" : "discarded",
		};
	}
	return null;
}
