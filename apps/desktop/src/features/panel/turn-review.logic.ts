import type { CoreEvent, TurnReviewFile } from "@dcc/contracts";

export type TurnReviewIdentity = {
	sessionId: string;
	workspaceId: string;
};

export type TurnReviewInvalidationCursor = {
	identity: string | null;
	terminalEvent: string | null;
};

export function reconcileTurnReviewSelection(
	selectedPath: string | null,
	files: TurnReviewFile[],
): string | null {
	if (selectedPath && files.some((file) => file.path === selectedPath)) {
		return selectedPath;
	}
	return (
		files.find((file) => !file.previewUnavailable)?.path ??
		files[0]?.path ??
		null
	);
}

export function latestTurnReviewTerminalEvent(
	events: CoreEvent[],
	sessionId: string,
): string | null {
	for (let index = events.length - 1; index >= 0; index -= 1) {
		const event = events[index];
		if (
			"sessionTurnCompleted" in event &&
			event.sessionTurnCompleted?.session_id === sessionId
		) {
			return `completed:${event.sessionTurnCompleted.turn_id}`;
		}
		if (
			"sessionTurnAborted" in event &&
			event.sessionTurnAborted?.session_id === sessionId
		) {
			return `aborted:${event.sessionTurnAborted.turn_id}`;
		}
	}
	return null;
}

export function shouldInvalidateTurnReview(
	previous: TurnReviewInvalidationCursor,
	next: TurnReviewInvalidationCursor,
): boolean {
	return Boolean(
		next.identity &&
			previous.identity === next.identity &&
			next.terminalEvent &&
			previous.terminalEvent !== next.terminalEvent,
	);
}

export function isTurnReviewIdentityActive(
	review: TurnReviewIdentity,
	active: TurnReviewIdentity | null,
): boolean {
	return Boolean(
		active &&
			review.sessionId === active.sessionId &&
			review.workspaceId === active.workspaceId,
	);
}

export type TurnReviewPreviewState =
	| "idle"
	| "loading"
	| "error"
	| "diff"
	| "unavailable";

export type TurnReviewOutcomePresentation = {
	outcome: "completed" | "aborted";
	reason: "interrupted" | "timeout" | "provider" | "other" | null;
};

export function resolveTurnReviewOutcome(
	turnOutcome: unknown,
	outcomeReason: unknown,
): TurnReviewOutcomePresentation | null {
	if (turnOutcome !== "completed" && turnOutcome !== "aborted") return null;
	if (turnOutcome === "completed") {
		return { outcome: "completed", reason: null };
	}
	if (typeof outcomeReason !== "string" || !outcomeReason.trim()) {
		return { outcome: "aborted", reason: "other" };
	}
	const normalized = outcomeReason.toLocaleLowerCase("en-US");
	if (/timeout|timed out|tempo limite|expir/.test(normalized)) {
		return { outcome: "aborted", reason: "timeout" };
	}
	if (/interrupt|abort|cancel|cancelad|interromp/.test(normalized)) {
		return { outcome: "aborted", reason: "interrupted" };
	}
	if (/provider|model|process|exit|failed|error|falh/.test(normalized)) {
		return { outcome: "aborted", reason: "provider" };
	}
	return { outcome: "aborted", reason: "other" };
}

export function resolveTurnReviewPreviewState(input: {
	selectedPath: string | null;
	isFetching: boolean;
	isError: boolean;
	diff: string | null | undefined;
}): TurnReviewPreviewState {
	if (!input.selectedPath) return "idle";
	if (input.isFetching) return "loading";
	if (input.isError) return "error";
	if (input.diff) return "diff";
	return "unavailable";
}
