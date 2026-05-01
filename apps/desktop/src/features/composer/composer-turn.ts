/**
 * Composer envelope sent to Tauri `send_turn`; **`compose_wire_prompt` runs in Rust**
 * (`dcc-core`) so Claude / Codex / Gemini / Cursor all receive the same directives on stdin.
 */

export type ComposerEffortLevel = "low" | "balanced" | "high";

export type ComposerTurnEnvelope = {
	/** Helmor-style planning phase: structured plan before edits / risky tools. */
	planMode: boolean;
	effort: ComposerEffortLevel;
	/** Short replies when true (Helmor fast prelude spirit). */
	fastMode: boolean;
};

export type ComposerSubmittedTurn = {
	/** Serialized composer text (includes @path badges). Shown in UI / pending bubble. */
	rawPrompt: string;
	envelope: ComposerTurnEnvelope;
};

export const DEFAULT_COMPOSER_ENVELOPE: ComposerTurnEnvelope = {
	planMode: false,
	effort: "balanced",
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
