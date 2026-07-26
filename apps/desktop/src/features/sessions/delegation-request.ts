import type { DelegationContextPolicy, DelegationMode } from "@dcc/contracts";

/**
 * A fully resolved delegation, ready to start. Every caller composes this from
 * something the user already did — a composer draft, an approved plan, a finished
 * delegation being replayed — rather than from a form.
 */
export type ManualDelegationRequest = {
	targetProviderId: string;
	targetProviderIds?: string[];
	targetModelId: string | null;
	mode: Extract<DelegationMode, "review" | "explain" | "implement">;
	contextPolicy: DelegationContextPolicy;
	instruction: string;
	/** Execution dials for the child turn; default to the historical medium/fast pair. */
	effort?: string;
	fastMode?: boolean;
	/**
	 * Sends this exact prompt instead of composing a new one. Used by reruns, which
	 * replay an earlier delegation verbatim so the comparison stays honest.
	 */
	prebuiltPrompt?: string;
};
