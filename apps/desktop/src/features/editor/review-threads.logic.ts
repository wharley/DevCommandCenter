import type { WorkspacePrReviewComment } from "@dcc/contracts";

export type ReviewThread = {
	id: string;
	comments: WorkspacePrReviewComment[];
};

/** open: still asks for something · outdated: its code changed · resolved: closed on the forge. */
export type ReviewThreadState = "open" | "outdated" | "resolved";

export function reviewCommentLine(comment: WorkspacePrReviewComment): number | null {
	const side = comment.side?.toUpperCase();
	if (side === "LEFT") {
		return comment.originalLine ?? comment.originalStartLine ?? comment.line;
	}
	return comment.line ?? comment.startLine ?? comment.originalLine;
}

export function reviewCommentStartLine(comment: WorkspacePrReviewComment): number | null {
	const side = comment.side?.toUpperCase();
	if (side === "LEFT") {
		return comment.originalStartLine ?? comment.originalLine ?? comment.line;
	}
	return comment.startLine ?? comment.line ?? comment.originalLine;
}

export function reviewCommentSide(comment: WorkspacePrReviewComment): "original" | "modified" {
	return comment.side?.toUpperCase() === "LEFT" ? "original" : "modified";
}

export function buildReviewThreads(comments: WorkspacePrReviewComment[]): ReviewThread[] {
	const grouped = new Map<string, WorkspacePrReviewComment[]>();
	for (const comment of comments) {
		const thread = grouped.get(comment.threadId);
		if (thread) {
			thread.push(comment);
		} else {
			grouped.set(comment.threadId, [comment]);
		}
	}
	return [...grouped.entries()]
		.map(([id, threadComments]) => ({
			id,
			comments: [...threadComments].sort((left, right) => {
				// The thread's root comes first even when timestamps are missing.
				if (left.parentId == null && right.parentId != null) return -1;
				if (right.parentId == null && left.parentId != null) return 1;
				const leftTime = Date.parse(left.createdAt ?? "");
				const rightTime = Date.parse(right.createdAt ?? "");
				return (Number.isNaN(leftTime) ? 0 : leftTime) - (Number.isNaN(rightTime) ? 0 : rightTime);
			}),
		}))
		.sort((left, right) => {
			const leftLine = reviewCommentLine(left.comments[0]!) ?? 0;
			const rightLine = reviewCommentLine(right.comments[0]!) ?? 0;
			return leftLine - rightLine;
		});
}

export function reviewThreadRoot(thread: ReviewThread): WorkspacePrReviewComment {
	return thread.comments.find((comment) => comment.parentId == null) ?? thread.comments[0]!;
}

export function reviewThreadState(thread: ReviewThread): ReviewThreadState {
	if (thread.comments.some((comment) => comment.resolved === true)) {
		return "resolved";
	}
	const root = reviewThreadRoot(thread);
	if (
		thread.comments.some((comment) => comment.outdated === true) ||
		reviewCommentLine(root) == null
	) {
		return "outdated";
	}
	return "open";
}

/** The forge can only resolve threads it reported as resolvable. */
export function reviewThreadResolvable(thread: ReviewThread): boolean {
	return thread.comments.some((comment) => comment.resolvable === true);
}

/**
 * Line numbers on a review comment point into the pull request's diff (base →
 * head), so only that diff can draw them, and only while the code still exists.
 */
export function reviewThreadAnchorable(
	thread: ReviewThread,
	showingPullRequestDiff: boolean,
): boolean {
	return showingPullRequestDiff && reviewThreadState(thread) === "open";
}

const STATE_ORDER: Record<ReviewThreadState, number> = {
	open: 0,
	outdated: 1,
	resolved: 2,
};

/** Open conversations first, in file order; closed ones sink to the bottom. */
export function sortReviewThreadsForList(threads: ReviewThread[]): ReviewThread[] {
	return [...threads].sort((left, right) => {
		const byState = STATE_ORDER[reviewThreadState(left)] - STATE_ORDER[reviewThreadState(right)];
		if (byState !== 0) return byState;
		return (reviewCommentLine(left.comments[0]!) ?? 0) - (reviewCommentLine(right.comments[0]!) ?? 0);
	});
}

/** What the file badges count: conversations still waiting on someone. */
export function countOpenReviewThreads(comments: WorkspacePrReviewComment[]): number {
	return buildReviewThreads(comments).filter(
		(thread) => reviewThreadState(thread) !== "resolved",
	).length;
}
