import { useCallback, useEffect, useRef, useState } from "react";

/** Identity is unique across local and external draft injections. */
export type ComposerPrefill = {
	requestId: string;
	text: string;
	nonce: number;
	mode?: "append" | "replace";
};
export type ComposerPrefillConsumption = Pick<
	ComposerPrefill,
	"requestId" | "text" | "nonce"
>;
export type ExternalComposerPrefill = Omit<ComposerPrefill, "requestId">;

export function useComposerPrefill({
	workspaceId,
	selectedSessionId,
	externalComposerPrefill,
	onExternalComposerPrefillConsumed,
}: {
	workspaceId: string;
	selectedSessionId?: string | null;
	externalComposerPrefill?: ExternalComposerPrefill | null;
	onExternalComposerPrefillConsumed?: (applied: ComposerPrefillConsumption) => void;
}) {
	const [composerPrefill, setComposerPrefill] = useState<ComposerPrefill | null>(null);
	const consumedExternalRequestIdRef = useRef<string | null>(null);

	useEffect(() => {
		// An unapplied request belongs to the workspace/conversation that produced it.
		setComposerPrefill(null);
	}, [selectedSessionId, workspaceId]);

	useEffect(() => {
		if (externalComposerPrefill) {
			const requestId = `external:${externalComposerPrefill.nonce}`;
			// A child can acknowledge the write before this parent effect runs.
			// App recreates the prop object on render, so a queued effect must not
			// reintroduce an already consumed request while state updates settle.
			if (consumedExternalRequestIdRef.current === requestId) return;
			setComposerPrefill((current) =>
				current?.requestId === requestId
					? current
					: { ...externalComposerPrefill, requestId },
			);
		}
	}, [externalComposerPrefill]);

	const handleComposerPrefillApplied = useCallback(
		(applied: ComposerPrefillConsumption) => {
			// Consume the panel's copy too: the editor plugin remounts when the
			// first send creates a session and must not replay the submitted draft.
			setComposerPrefill((current) =>
				current?.requestId === applied.requestId ? null : current,
			);
			if (
				applied.requestId === `external:${externalComposerPrefill?.nonce}` &&
				externalComposerPrefill?.text === applied.text
			) {
				consumedExternalRequestIdRef.current = applied.requestId;
				onExternalComposerPrefillConsumed?.(applied);
			}
		},
		[externalComposerPrefill, onExternalComposerPrefillConsumed],
	);

	return { composerPrefill, setComposerPrefill, handleComposerPrefillApplied };
}
