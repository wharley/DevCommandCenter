/**
 * Composer envelope sent to Tauri `send_turn`; Rust forwards this as structured
 * turn metadata and each provider adapter decides what travels natively vs. via
 * provider-local prompt fallback.
 */

export type ComposerEffortLevel = string;

export type ComposerTurnEnvelope = {
	/** Planning phase: structured plan before edits or risky tools. */
	planMode: boolean;
	effort: ComposerEffortLevel;
	/** Short replies when true. */
	fastMode: boolean;
};

export type ComposerSubmittedTurn = {
	/** Serialized composer text (includes @path badges). Shown in UI / pending bubble. */
	rawPrompt: string;
	envelope: ComposerTurnEnvelope;
};

/**
 * Delegation intent emitted by the composer. It carries only what the user
 * actually chose — target(s), write permission, and the same effort/fast dials
 * used for a normal turn. Mode and context policy are derived downstream.
 */
export type ComposerDelegationRequest = {
	rawPrompt: string;
	targetProviderIds: string[];
	allowFileEdits: boolean;
	effort: ComposerEffortLevel;
	fastMode: boolean;
};

export const DEFAULT_COMPOSER_ENVELOPE: ComposerTurnEnvelope = {
	planMode: false,
	effort: "medium",
	fastMode: true,
};

export function composerTurnFromRaw(
	rawPrompt: string,
	overrides?: Partial<ComposerTurnEnvelope>,
): ComposerSubmittedTurn {
	return {
		rawPrompt,
		envelope: { ...DEFAULT_COMPOSER_ENVELOPE, ...overrides },
	};
}
