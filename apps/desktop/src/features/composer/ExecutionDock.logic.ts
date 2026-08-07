export type ExecutionDockChangeSummary = {
	/** Files changed in the working tree (staged or unstaged). */
	files: number;
	additions: number;
	deletions: number;
	/** Files changed by commits on the current branch against its base. */
	branchFiles: number;
	branchAdditions: number;
	branchDeletions: number;
	aheadOfRemoteCount: number;
	pullRequestState?: string | null;
	pullRequestNumber?: number | null;
};

export type ExecutionDockGitState = "loading" | "ready" | "error";

export type ExecutionDockStatus =
	| { kind: "loading" | "error" | "none" }
	| { kind: "merged"; pullRequestNumber: number | null }
	| {
			kind: "local";
			files: number;
			additions: number;
			deletions: number;
		}
	| {
			kind: "branch";
			files: number;
			additions: number;
			deletions: number;
		}
	| {
			kind: "local-and-branch";
			localFiles: number;
			branchFiles: number;
		}
	| { kind: "ahead"; commits: number };

export function resolveExecutionDockStatus(
	summary: ExecutionDockChangeSummary | null,
	gitState: ExecutionDockGitState,
): ExecutionDockStatus {
	if (gitState === "loading") return { kind: "loading" };
	if (gitState === "error") return { kind: "error" };
	if (!summary) return { kind: "none" };

	const hasLocalChanges = summary.files > 0;
	const hasBranchChanges = summary.branchFiles > 0;
	const isMerged = summary.pullRequestState?.trim().toLowerCase() === "merged";

	// Never hide uncommitted work behind a delivery status.
	if (hasLocalChanges && hasBranchChanges) {
		return {
			kind: "local-and-branch",
			localFiles: summary.files,
			branchFiles: summary.branchFiles,
		};
	}
	if (hasLocalChanges) {
		return {
			kind: "local",
			files: summary.files,
			additions: summary.additions,
			deletions: summary.deletions,
		};
	}
	if (isMerged) {
		return {
			kind: "merged",
			pullRequestNumber: summary.pullRequestNumber ?? null,
		};
	}
	if (hasBranchChanges) {
		return {
			kind: "branch",
			files: summary.branchFiles,
			additions: summary.branchAdditions,
			deletions: summary.branchDeletions,
		};
	}
	if (summary.aheadOfRemoteCount > 0) {
		return { kind: "ahead", commits: summary.aheadOfRemoteCount };
	}
	return { kind: "none" };
}
