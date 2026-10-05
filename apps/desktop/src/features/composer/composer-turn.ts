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
	/** User-selected approval boundary, normalized across supported providers. */
	approvalPolicy: ProviderApprovalPolicy | null;
	/** Metadata-only summary of the evidence composed into `rawPrompt`; never bodies. */
	evidence?: TurnEvidenceSummary | null;
};

export type ComposerSubmittedTurn = {
	/** Serialized composer text (includes @path badges). Shown in UI / pending bubble. */
	rawPrompt: string;
	envelope: ComposerTurnEnvelope;
};

export const DEFAULT_COMPOSER_ENVELOPE: ComposerTurnEnvelope = {
	planMode: false,
	effort: "medium",
	fastMode: false,
	approvalPolicy: null,
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
import type { ProviderApprovalPolicy, TurnEvidenceSummary } from "@dcc/contracts";
