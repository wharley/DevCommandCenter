import {
	commitTranslationKey,
	type CommitMode,
} from "@/features/commit/WorkspaceCommitButton.logic";

export type WorkspaceRecapTone = "neutral" | "working" | "attention" | "ready" | "done";

export type WorkspaceDeliveryState =
	| "in_development"
	| "needs_attention"
	| "blocked"
	| "awaiting_review"
	| "ready_to_deliver"
	| "delivered";

export type WorkspaceDeliverySignalState =
	| "passed"
	| "pending"
	| "attention"
	| "blocked"
	| "unavailable";

export type WorkspaceDeliverySignal = {
	id: "workspace" | "recovery" | "agent_review" | "review" | "pipeline" | "checks";
	state: WorkspaceDeliverySignalState;
	messageKey: string;
	params: Record<string, string | number>;
	required: boolean;
};

export type WorkspaceRecapActionKind =
	| "git"
	| "review"
	| "review-state"
	| "pipeline"
	| "delivery"
	| "sync"
	| "automation"
	| "activity";

export type WorkspaceRecapAction = {
	kind: WorkspaceRecapActionKind;
	/** i18n key under namespace `common` (accepts a `requestLabel` param). */
	labelKey: string;
};

export type WorkspaceRecapMode = "git" | "code";

export type WorkspaceRecap = {
	/** Derived Delivery Status; never persisted as an independent source of truth. */
	state: WorkspaceDeliveryState;
	/** i18n key under `common:inspector.recap.messages`. */
	messageKey: string;
	params: Record<string, string | number>;
	tone: WorkspaceRecapTone;
	action: WorkspaceRecapAction | null;
	signals: WorkspaceDeliverySignal[];
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
	codeRabbitReviewAvailable?: boolean;
	/** Completed delegation results that still deserve human review. */
	pendingDelegationResultsCount: number;
	deliveryFailure?: {
		operation: "fetch" | "pull" | "push" | "pipeline";
		classification: string;
	} | null;
	reviewState?: {
		reviewState: string | null;
		approvalsAvailable: boolean;
		approvalsReceived: number;
		approvalsLeft: number | null;
		hasConflicts: boolean | null;
		behindBy: number | null;
		discussionsResolved: boolean | null;
		draft: boolean | null;
	} | null;
	reviewStatus?: "idle" | "loading" | "error" | "available";
	pipeline?: {
		status: string;
		failedJobs: number;
	} | null;
	pipelineStatus?: "idle" | "loading" | "error" | "available";
	deliveryPolicy?: {
		minimumApprovals: number;
		requirePipeline: boolean;
		requireResolvedDiscussions: boolean;
		requireCurrentBase: boolean;
		requireBeforeMergeChecks: boolean;
	} | null;
	beforeMergeChecksCount?: number;
};

function gitAction(mode: CommitMode): WorkspaceRecapAction {
	return { kind: "git", labelKey: commitTranslationKey(mode, "idle") };
}

/**
 * Keeps Git workflow actions in the Git header, where their state and
 * consequences are visible. Outside Changes, the recap may navigate to a Git
 * mutation or expose a transition that would otherwise be unreachable; inside
 * Changes, repeating either CTA would create a false second decision point.
 */
export function workspaceRecapActionForMode(
	action: WorkspaceRecapAction | null,
	mode: WorkspaceRecapMode,
): WorkspaceRecapAction | null {
	if (!action) {
		return action;
	}
	if (mode === "git" && action.kind === "git") {
		return null;
	}
	if (action.kind === "git") {
		return { kind: "git", labelKey: "inspector.recap.actions.changes" };
	}
	return action;
}

function prRef(input: WorkspaceRecapInput): string {
	return input.prNumber
		? `${input.requestLabel} #${input.prNumber}`
		: input.requestLabel;
}

function sectionAction(
	kind: "delivery" | "pipeline" | "review-state",
	labelKey: string,
): WorkspaceRecapAction {
	return { kind, labelKey };
}

/**
 * One-sentence answer to "where is this workspace and what now?", always paired
 * with the action that moves it forward. The next-action decision is
 * `resolveCommitMode` — this layers the states it cannot see (active agent
 * turn, captured failures, review, approvals, and pipeline) on top; it never
 * persists or invents a second Git state machine.
 */
function buildWorkspaceRecapPrimary(
	input: WorkspaceRecapInput,
): Omit<WorkspaceRecap, "signals"> {
	if (input.commitMode === "complete-merge") {
		return {
			state: "ready_to_deliver",
			messageKey: "conflictResolutionReady",
			params: { count: input.conflictCount },
			tone: "ready",
			action: gitAction("complete-merge"),
		};
	}

	if (input.commitMode === "resolve-conflicts" || input.conflictCount > 0) {
		return {
			state: "blocked",
			messageKey: "conflicts",
			params: { count: Math.max(input.conflictCount, 1), pr: prRef(input) },
			tone: "attention",
			action: gitAction("resolve-conflicts"),
		};
	}

	if (input.commitMode === "merged") {
		return {
			state: "delivered",
			messageKey: "merged",
			params: { pr: prRef(input) },
			tone: "done",
			action: null,
		};
	}

	if (
		input.commitMode === "closed" ||
		(input.commitMode === "open-pr" && input.prState?.toLowerCase() === "closed")
	) {
		return {
			state: "needs_attention",
			messageKey: "closed",
			params: { pr: prRef(input) },
			tone: "neutral",
			action: input.prNumber ? gitAction("open-pr") : null,
		};
	}

	if (input.turnRunning) {
		return {
			state: "in_development",
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

	if (input.deliveryFailure) {
		const state =
			input.deliveryFailure.classification === "authentication" ||
			input.deliveryFailure.classification === "transport" ||
			input.deliveryFailure.classification === "unknown"
				? "needs_attention"
				: "blocked";
		return {
			state,
			messageKey: `deliveryFailure.${input.deliveryFailure.operation}`,
			params: {},
			tone: "attention",
			action: sectionAction(
				"delivery",
				"inspector.recap.actions.delivery",
			),
		};
	}

	if (input.pendingDelegationResultsCount > 0) {
		return {
			state: "needs_attention",
			messageKey: "delegationResults",
			params: { count: input.pendingDelegationResultsCount },
			tone: "ready",
			action: { kind: "activity", labelKey: "inspector.recap.actions.activity" },
		};
	}

	if (input.pendingReviewFindingsCount > 0) {
		return {
			state: "needs_attention",
			messageKey: "findings",
			params: { count: input.pendingReviewFindingsCount },
			tone: "attention",
			action: { kind: "review", labelKey: "inspector.recap.actions.review" },
		};
	}

	if (input.commitMode === "commit-and-push") {
		return {
			state: "in_development",
			messageKey: "changes",
			params: {
				count: input.changedFilesCount,
				additions: input.additions,
				deletions: input.deletions,
			},
			tone: "neutral",
			action: gitAction("commit-and-push"),
		};
	}

	if (input.commitMode === "push") {
		return {
			state: "in_development",
			messageKey: "ahead",
			params: { count: input.aheadOfRemoteCount },
			tone: "neutral",
			action: gitAction("push"),
		};
	}

	if (input.reviewState?.hasConflicts === true) {
		return {
			state: "blocked",
			messageKey: "reviewConflicts",
			params: { pr: prRef(input) },
			tone: "attention",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if (input.reviewState?.reviewState === "changes_requested") {
		return {
			state: "blocked",
			messageKey: "changesRequested",
			params: { pr: prRef(input) },
			tone: "attention",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if (input.pipeline?.status === "failed") {
		return {
			state: "blocked",
			messageKey: "pipelineFailed",
			params: { count: Math.max(input.pipeline.failedJobs, 1) },
			tone: "attention",
			action: sectionAction(
				"pipeline",
				"inspector.recap.actions.pipeline",
			),
		};
	}

	if (input.reviewState?.discussionsResolved === false) {
		return {
			state: "needs_attention",
			messageKey: "discussionsOpen",
			params: { pr: prRef(input) },
			tone: "attention",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if ((input.reviewState?.behindBy ?? 0) > 0) {
		return {
			state: "needs_attention",
			messageKey: "behindBase",
			params: { count: input.reviewState?.behindBy ?? 0 },
			tone: "attention",
			action: {
				kind: "sync",
				labelKey: "inspector.recap.actions.synchronize",
			},
		};
	}

	if (input.pipeline?.status === "canceled") {
		return {
			state: "needs_attention",
			messageKey: "pipelineCanceled",
			params: {},
			tone: "attention",
			action: sectionAction(
				"pipeline",
				"inspector.recap.actions.pipeline",
			),
		};
	}

	if (input.pipeline?.status === "manual") {
		return {
			state: "needs_attention",
			messageKey: "pipelineManual",
			params: {},
			tone: "attention",
			action: sectionAction(
				"pipeline",
				"inspector.recap.actions.pipeline",
			),
		};
	}

	if (input.prNumber && input.reviewStatus === "loading") {
		return {
			state: "awaiting_review",
			messageKey: "reviewLoading",
			params: { pr: prRef(input) },
			tone: "working",
			action: null,
		};
	}

	if (input.pipelineStatus === "loading") {
		return {
			state: "awaiting_review",
			messageKey: "pipelineLoading",
			params: {},
			tone: "working",
			action: null,
		};
	}

	if (input.reviewStatus === "error") {
		return {
			state: "needs_attention",
			messageKey: "reviewUnavailable",
			params: { pr: prRef(input) },
			tone: "attention",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if (input.pipelineStatus === "error") {
		return {
			state: "needs_attention",
			messageKey: "pipelineUnavailable",
			params: {},
			tone: "attention",
			action: sectionAction(
				"pipeline",
				"inspector.recap.actions.pipeline",
			),
		};
	}

	if (
		input.deliveryPolicy?.requireResolvedDiscussions &&
		input.prNumber &&
		input.reviewState?.discussionsResolved == null
	) {
		return {
			state: "needs_attention",
			messageKey: "policy.discussionsUnavailable",
			params: { pr: prRef(input) },
			tone: "attention",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if (
		input.deliveryPolicy?.requireCurrentBase &&
		input.prNumber &&
		input.reviewState?.behindBy == null
	) {
		return {
			state: "needs_attention",
			messageKey: "policy.baseUnavailable",
			params: { pr: prRef(input) },
			tone: "attention",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if (input.reviewState?.draft === true) {
		return {
			state: "awaiting_review",
			messageKey: "draft",
			params: { pr: prRef(input) },
			tone: "neutral",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if (
		input.prNumber &&
		input.reviewState &&
		!input.reviewState.approvalsAvailable
	) {
		return {
			state: "awaiting_review",
			messageKey: "approvalsUnavailable",
			params: { pr: prRef(input) },
			tone: "neutral",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if ((input.reviewState?.approvalsLeft ?? 0) > 0) {
		return {
			state: "awaiting_review",
			messageKey: "approvalsPending",
			params: { count: input.reviewState?.approvalsLeft ?? 0 },
			tone: "neutral",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if (
		input.prNumber &&
		(input.deliveryPolicy?.minimumApprovals ?? 0) >
		(input.reviewState?.approvalsReceived ?? 0)
	) {
		return {
			state: "awaiting_review",
			messageKey: "policy.approvalsPending",
			params: {
				count:
					(input.deliveryPolicy?.minimumApprovals ?? 0) -
					(input.reviewState?.approvalsReceived ?? 0),
				required: input.deliveryPolicy?.minimumApprovals ?? 0,
			},
			tone: "neutral",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if (
		input.reviewState?.reviewState === "pending" ||
		input.reviewState?.reviewState === "unknown"
	) {
		return {
			state: "awaiting_review",
			messageKey: "reviewPending",
			params: { pr: prRef(input) },
			tone: "neutral",
			action: sectionAction(
				"review-state",
				"inspector.recap.actions.reviewState",
			),
		};
	}

	if (
		input.pipeline &&
		[
			"created",
			"waiting_for_resource",
			"preparing",
			"pending",
			"running",
			"scheduled",
		].includes(input.pipeline.status)
	) {
		return {
			state: "awaiting_review",
			messageKey: "pipelineRunning",
			params: {},
			tone: "working",
			action: sectionAction(
				"pipeline",
				"inspector.recap.actions.pipeline",
			),
		};
	}

	if (input.prNumber && input.deliveryPolicy?.requirePipeline) {
		if (!input.pipeline) {
			return {
				state: "needs_attention",
				messageKey: "policy.pipelineMissing",
				params: {},
				tone: "attention",
				action:
					input.pipelineStatus === "available"
						? sectionAction(
								"pipeline",
								"inspector.recap.actions.pipeline",
							)
						: {
								kind: "automation",
								labelKey: "inspector.recap.actions.policy",
							},
			};
		}
		if (input.pipeline.status !== "success") {
			return {
				state: "needs_attention",
				messageKey: "policy.pipelineNotSuccessful",
				params: {},
				tone: "attention",
				action: sectionAction(
					"pipeline",
					"inspector.recap.actions.pipeline",
				),
			};
		}
	}

	if (
		input.prNumber &&
		input.deliveryPolicy?.requireBeforeMergeChecks &&
		(input.beforeMergeChecksCount ?? 0) === 0
	) {
		return {
			state: "needs_attention",
			messageKey: "policy.checksMissing",
			params: {},
			tone: "attention",
			action: {
				kind: "automation",
				labelKey: "inspector.recap.actions.policy",
			},
		};
	}

	switch (input.commitMode) {
		case "fix":
			return {
				state: "blocked",
				messageKey: "checksFailing",
				params: { pr: prRef(input) },
				tone: "attention",
				action: gitAction("fix"),
			};
		case "merge":
			return {
				state: "ready_to_deliver",
				messageKey: "mergeReady",
				params: { pr: prRef(input) },
				tone: "ready",
				action: gitAction("merge"),
			};
		case "open-pr":
			return {
				state: "awaiting_review",
				messageKey: "prOpen",
				params: { pr: prRef(input) },
				tone: "neutral",
				action: gitAction("open-pr"),
			};
		case "create-pr":
			if (input.committedVsBaseCount > 0) {
				return {
					state: "in_development",
					messageKey: "readyForPr",
					params: {
						count: input.committedVsBaseCount,
						requestLabel: input.requestLabel,
					},
					tone: "ready",
					action: gitAction("create-pr"),
				};
			}
			return {
				state: "in_development",
				messageKey: "clean",
				params: {},
				tone: "neutral",
				action: null,
			};
		default:
			return {
				state: "in_development",
				messageKey: "clean",
				params: {},
				tone: "neutral",
				action: null,
			};
	}
}

function buildWorkspaceDeliverySignals(
	input: WorkspaceRecapInput,
): WorkspaceDeliverySignal[] {
	const policy = input.deliveryPolicy;
	const reviewRequired =
		(policy?.minimumApprovals ?? 0) > 0 ||
		Boolean(policy?.requireResolvedDiscussions) ||
		Boolean(policy?.requireCurrentBase);
	const pipelineRequired = Boolean(policy?.requirePipeline);
	const checksRequired = Boolean(policy?.requireBeforeMergeChecks);
	const signals: WorkspaceDeliverySignal[] = [];

	if (input.commitMode === "complete-merge") {
		signals.push({
			id: "workspace",
			state: "pending",
			messageKey: "conflictResolutionReady",
			params: { count: input.conflictCount },
			required: false,
		});
	} else if (input.conflictCount > 0 || input.commitMode === "resolve-conflicts") {
		signals.push({
			id: "workspace",
			state: "blocked",
			messageKey: "workspaceConflicts",
			params: { count: Math.max(input.conflictCount, 1) },
			required: false,
		});
	} else if (input.changedFilesCount > 0) {
		signals.push({
			id: "workspace",
			state: "pending",
			messageKey: "workspaceChanges",
			params: { count: input.changedFilesCount },
			required: false,
		});
	} else if (input.aheadOfRemoteCount > 0) {
		signals.push({
			id: "workspace",
			state: "pending",
			messageKey: "workspaceAhead",
			params: { count: input.aheadOfRemoteCount },
			required: false,
		});
	} else {
		signals.push({
			id: "workspace",
			state: "passed",
			messageKey: "workspaceClean",
			params: {},
			required: false,
		});
	}

	signals.push(
		input.deliveryFailure
			? {
					id: "recovery",
					state:
						input.deliveryFailure.classification === "authentication" ||
						input.deliveryFailure.classification === "transport" ||
						input.deliveryFailure.classification === "unknown"
							? "attention"
							: "blocked",
					messageKey: "recoveryCaptured",
					params: {},
					required: false,
				}
			: {
					id: "recovery",
					state: "passed",
					messageKey: "recoveryClear",
					params: {},
					required: false,
				},
	);

	if (input.pendingReviewFindingsCount > 0) {
		signals.push({
			id: "agent_review",
			state: "attention",
			messageKey: "agentFindings",
			params: { count: input.pendingReviewFindingsCount },
			required: false,
		});
	} else if (input.codeRabbitReviewAvailable) {
		signals.push({
			id: "agent_review",
			state: "passed",
			messageKey: "agentReviewClear",
			params: {},
			required: false,
		});
	} else {
		signals.push({
			id: "agent_review",
			state: "unavailable",
			messageKey: "agentReviewUnavailable",
			params: {},
			required: false,
		});
	}

	if (!input.prNumber) {
		signals.push({
			id: "review",
			state: "unavailable",
			messageKey: "reviewNotStarted",
			params: {},
			required: reviewRequired,
		});
	} else if (input.reviewStatus === "loading") {
		signals.push({
			id: "review",
			state: "pending",
			messageKey: "reviewLoading",
			params: {},
			required: reviewRequired,
		});
	} else if (input.reviewStatus === "error" || !input.reviewState) {
		signals.push({
			id: "review",
			state: "unavailable",
			messageKey: "reviewUnavailable",
			params: {},
			required: reviewRequired,
		});
	} else {
		const review = input.reviewState;
		const approvalDeficit = Math.max(
			0,
			(policy?.minimumApprovals ?? 0) - review.approvalsReceived,
		);
		let state: WorkspaceDeliverySignalState = "passed";
		let messageKey = "reviewPassed";
		let params: Record<string, string | number> = {
			count: review.approvalsReceived,
		};
		if (review.hasConflicts || review.reviewState === "changes_requested") {
			state = "blocked";
			messageKey = "reviewBlocked";
			params = {};
		} else if (
			review.discussionsResolved === false ||
			(review.behindBy ?? 0) > 0
		) {
			state = "attention";
			messageKey = "reviewAttention";
			params = {};
		} else if (!review.approvalsAvailable && reviewRequired) {
			state = "unavailable";
			messageKey = "reviewUnavailable";
			params = {};
		} else if (
			approvalDeficit > 0 ||
			review.draft ||
			review.reviewState === "pending"
		) {
			state = "pending";
			messageKey = "reviewPending";
			params = { count: approvalDeficit };
		}
		signals.push({
			id: "review",
			state,
			messageKey,
			params,
			required: reviewRequired,
		});
	}

	if (input.pipelineStatus === "loading") {
		signals.push({
			id: "pipeline",
			state: "pending",
			messageKey: "pipelineLoading",
			params: {},
			required: pipelineRequired,
		});
	} else if (input.pipelineStatus === "error") {
		signals.push({
			id: "pipeline",
			state: "unavailable",
			messageKey: "pipelineUnavailable",
			params: {},
			required: pipelineRequired,
		});
	} else if (!input.pipeline) {
		signals.push({
			id: "pipeline",
			state: "unavailable",
			messageKey:
				input.pipelineStatus === "idle"
					? "pipelineNotIntegrated"
					: "pipelineMissing",
			params: {},
			required: pipelineRequired,
		});
	} else {
		const status = input.pipeline.status;
		const active = [
			"created",
			"waiting_for_resource",
			"preparing",
			"pending",
			"running",
			"scheduled",
		].includes(status);
		const state: WorkspaceDeliverySignalState =
			status === "success"
				? "passed"
				: status === "failed"
					? "blocked"
					: active
						? "pending"
						: "attention";
		signals.push({
			id: "pipeline",
			state,
			messageKey:
				status === "success"
					? "pipelinePassed"
					: status === "failed"
						? "pipelineFailed"
						: active
							? "pipelineRunning"
							: "pipelineAttention",
			params: {},
			required: pipelineRequired,
		});
	}

	const beforeMergeChecksCount = input.beforeMergeChecksCount ?? 0;
	signals.push({
		id: "checks",
		state:
			beforeMergeChecksCount > 0
				? "passed"
				: checksRequired
					? "blocked"
					: "unavailable",
		messageKey:
			beforeMergeChecksCount > 0
				? "checksConfigured"
				: checksRequired
					? "checksMissing"
					: "checksOptional",
		params: { count: beforeMergeChecksCount },
		required: checksRequired,
	});

	return signals;
}

export function buildWorkspaceRecap(input: WorkspaceRecapInput): WorkspaceRecap {
	return {
		...buildWorkspaceRecapPrimary(input),
		signals: buildWorkspaceDeliverySignals(input),
	};
}
