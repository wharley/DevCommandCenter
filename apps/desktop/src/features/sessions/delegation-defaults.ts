import type { DelegationContextPolicy, DelegationMode } from "@dcc/contracts";

export type DerivedDelegationMode = Extract<
	DelegationMode,
	"review" | "explain" | "implement"
>;

export type DerivedDelegationDefaults = {
	mode: DerivedDelegationMode;
	contextPolicy: DelegationContextPolicy;
};

/**
 * Mode and context policy are derived, never asked.
 *
 * `mode` only drives edit permission, the child thread title, and the wording of
 * the delegation prompt — it is not part of any provider protocol. So the single
 * real decision ("can this agent write files?") plus the presence of a working
 * tree diff is enough to pick a sane mode/context pair, and `contextPolicy` never
 * has to enter the user's vocabulary.
 */
export function resolveDelegationDefaults(input: {
	allowFileEdits: boolean;
	hasWorkingTreeChanges: boolean;
}): DerivedDelegationDefaults {
	if (input.allowFileEdits) {
		// Implementation delegations start from a clean tree (preflight enforces it),
		// so the child needs the session narrative rather than a diff.
		return { mode: "implement", contextPolicy: { type: "full_reanchor" } };
	}
	if (input.hasWorkingTreeChanges) {
		return { mode: "review", contextPolicy: { type: "review_current_diff" } };
	}
	// Nothing changed yet: there is no work to review, only code to explain.
	return { mode: "explain", contextPolicy: { type: "minimal" } };
}
