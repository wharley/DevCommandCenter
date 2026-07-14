import {
	commitTranslationKey,
	type CommitMode,
} from "@/features/commit/WorkspaceCommitButton.logic";

export type WorkspaceRecapTone = "neutral" | "working" | "attention" | "ready" | "done";

export type WorkspaceRecapActionKind = "git" | "review" | "activity" | "continue";

export type WorkspaceRecapAction = {
	kind: WorkspaceRecapActionKind;
	/** i18n key under namespace `common` (accepts a `requestLabel` param). */
	labelKey: string;
};

export type WorkspaceRecap = {
	/** i18n key under `common:inspector.recap.messages`. */
	messageKey: string;
	params: Record<string, string | number>;
	tone: WorkspaceRecapTone;
	action: WorkspaceRecapAction | null;
};

export type WorkspaceRecapInput = {
	commitMode: CommitMode;
	/** A turn is currently in flight (the session itself may remain active between turns). */
	turnRunning: boolean;
	changedFilesCount: number;
	additions: number;
	deletions: number;
	aheadOfRemoteCount: number;
	conflictCount: number;
	/** Files committed on this branch vs the base branch. */
	committedVsBaseCount: number;
	prNumber: number | null;
	prState: string | null;
	requestLabel: "PR" | "MR";
	/** CodeRabbit findings whose review still matches the current diff. */
	pendingReviewFindingsCount: number;
	/** Completed delegation results that still deserve human review. */
	pendingDelegationResultsCount: number;
};

function gitAction(mode: CommitMode): WorkspaceRecapAction {
	return { kind: "git", labelKey: commitTranslationKey(mode, "idle") };
}

function prRef(input: WorkspaceRecapInput): string {
	return input.prNumber
		? `${input.requestLabel} #${input.prNumber}`
		: input.requestLabel;
}

/**
 * One-sentence answer to "where is this workspace and what now?", always paired
 * with the action that moves it forward. The next-action decision is
 * `resolveCommitMode` — this only layers the states it can't see (agent
 * running turn, pending review findings) on top; it never invents a second
 * git state machine.
 */
export function buildWorkspaceRecap(input: WorkspaceRecapInput): WorkspaceRecap {
	if (input.commitMode === "resolve-conflicts" || input.conflictCount > 0) {
		return {
			messageKey: "conflicts",
			params: { count: Math.max(input.conflictCount, 1), pr: prRef(input) },
			tone: "attention",
			action: gitAction("resolve-conflicts"),
		};
	}

	if (input.commitMode === "merged") {
		return {
			messageKey: "merged",
			params: { pr: prRef(input) },
			tone: "done",
			action: { kind: "continue", labelKey: "inspector.recap.actions.continue" },
		};
	}

	if (
		input.commitMode === "closed" ||
		(input.commitMode === "open-pr" && input.prState?.toLowerCase() === "closed")
	) {
		return {
			messageKey: "closed",
			params: { pr: prRef(input) },
			tone: "neutral",
			action: input.prNumber ? gitAction("open-pr") : null,
		};
	}

	if (input.turnRunning) {
		return {
			messageKey: input.changedFilesCount > 0 ? "working" : "workingClean",
			params: {
				count: input.changedFilesCount,
				additions: input.additions,
				deletions: input.deletions,
			},
			tone: "working",
			action: { kind: "activity", labelKey: "inspector.recap.actions.activity" },
		};
	}

	if (input.pendingDelegationResultsCount > 0) {
		return {
			messageKey: "delegationResults",
			params: { count: input.pendingDelegationResultsCount },
			tone: "ready",
			action: { kind: "activity", labelKey: "inspector.recap.actions.activity" },
		};
	}

	if (input.pendingReviewFindingsCount > 0) {
		return {
			messageKey: "findings",
			params: { count: input.pendingReviewFindingsCount },
			tone: "attention",
			action: { kind: "review", labelKey: "inspector.recap.actions.review" },
		};
	}

	switch (input.commitMode) {
		case "commit-and-push":
			return {
				messageKey: "changes",
				params: {
					count: input.changedFilesCount,
					additions: input.additions,
					deletions: input.deletions,
				},
				tone: "neutral",
				action: gitAction("commit-and-push"),
			};
		case "push":
			return {
				messageKey: "ahead",
				params: { count: input.aheadOfRemoteCount },
				tone: "neutral",
				action: gitAction("push"),
			};
		case "fix":
			return {
				messageKey: "checksFailing",
				params: { pr: prRef(input) },
				tone: "attention",
				action: gitAction("fix"),
			};
		case "merge":
			return {
				messageKey: "mergeReady",
				params: { pr: prRef(input) },
				tone: "ready",
				action: gitAction("merge"),
			};
		case "open-pr":
			return {
				messageKey: "prOpen",
				params: { pr: prRef(input) },
				tone: "neutral",
				action: gitAction("open-pr"),
			};
		case "create-pr":
			if (input.committedVsBaseCount > 0) {
				return {
					messageKey: "readyForPr",
					params: {
						count: input.committedVsBaseCount,
						requestLabel: input.requestLabel,
					},
					tone: "ready",
					action: gitAction("create-pr"),
				};
			}
			return { messageKey: "clean", params: {}, tone: "neutral", action: null };
		default:
			return { messageKey: "clean", params: {}, tone: "neutral", action: null };
	}
}
