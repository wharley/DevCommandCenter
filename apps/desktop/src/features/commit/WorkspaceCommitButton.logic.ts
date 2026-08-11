export type CommitMode =
	| "create-pr"
	| "open-pr"
	| "commit-and-push"
	| "push"
	| "fix"
	| "resolve-conflicts"
	| "complete-merge"
	| "merge"
	| "merged"
	| "closed";

export type CommitButtonStatus = "idle" | "busy" | "done" | "error";

export type WorkspaceGitStatusSummary = {
	staged: unknown[];
	unstaged: unknown[];
	currentBranch?: string | null;
	aheadOfRemoteCount?: number;
	behindOfRemoteCount?: number;
	conflictCount?: number;
	mergeInProgress?: boolean;
	conflictResolutionReady?: boolean;
};

export type WorkspacePrStatusSummary = {
	provider?: string | null;
	host?: string | null;
	number?: number | null;
	title?: string | null;
	url?: string | null;
	headBranch?: string | null;
	baseBranch?: string | null;
	state?: string | null;
	mergeable?: string | null;
	mergeStateStatus?: string | null;
	isDraft?: boolean;
};

export type CommitModeContext = {
	branch: string;
	prStatus?: WorkspacePrStatusSummary | null;
	gitStatus?: WorkspaceGitStatusSummary | null;
};

function hasWorkingTreeChanges(gitStatus?: WorkspaceGitStatusSummary | null) {
	return (gitStatus?.staged.length ?? 0) > 0 || (gitStatus?.unstaged.length ?? 0) > 0;
}

function hasFailingChecks(prStatus?: WorkspacePrStatusSummary | null) {
	const mergeStateStatus = prStatus?.mergeStateStatus?.toUpperCase();
	return mergeStateStatus === "DIRTY" || mergeStateStatus === "BLOCKED" || mergeStateStatus === "UNSTABLE";
}

function resolveFromContext(context: CommitModeContext) {
	const normalized = context.branch.toLowerCase();
	const prHeadBranch = context.prStatus?.headBranch;
	const prBelongsHere = !prHeadBranch || prHeadBranch === context.branch;
	const prState = prBelongsHere ? context.prStatus?.state?.toLowerCase() : undefined;

	if (context.gitStatus?.mergeInProgress) {
		if (
			(context.gitStatus.conflictCount ?? 0) === 0 ||
			context.gitStatus.conflictResolutionReady
		) {
			return "complete-merge" as const;
		}
		return "resolve-conflicts" as const;
	}
	if ((context.gitStatus?.conflictCount ?? 0) > 0) {
		return "resolve-conflicts" as const;
	}
	if (prState === "merged") return "merged" as const;
	if (prState === "closed") return "open-pr" as const;

	if (prState === "open") {
		if (hasWorkingTreeChanges(context.gitStatus)) {
			return "commit-and-push" as const;
		}
		if ((context.gitStatus?.aheadOfRemoteCount ?? 0) > 0) {
			return "push" as const;
		}
		// The forge keeps reporting CONFLICTING until a local resolution is
		// committed and pushed. Local Git state must take precedence so that the
		// Inspector can complete that resolution flow.
		if (context.prStatus?.mergeable === "CONFLICTING") {
			return "resolve-conflicts" as const;
		}
		if (hasFailingChecks(context.prStatus)) {
			return "fix" as const;
		}
		return "merge" as const;
	}

	if (hasWorkingTreeChanges(context.gitStatus)) {
		return "commit-and-push" as const;
	}
	if ((context.gitStatus?.aheadOfRemoteCount ?? 0) > 0) {
		return "push" as const;
	}

	if (normalized.includes("closed")) return "closed" as const;
	if (normalized.includes("merged")) return "merged" as const;
	if (normalized.includes("conflict")) return "resolve-conflicts" as const;
	if (normalized.includes("merge")) return "merge" as const;
	if (normalized.includes("fix")) return "fix" as const;
	if (normalized.includes("pr/")) return "open-pr" as const;
	if (normalized.includes("push")) return "push" as const;
	return "create-pr" as const;
}

export function resolveCommitMode(input: string): CommitMode;
export function resolveCommitMode(input: CommitModeContext): CommitMode;
export function resolveCommitMode(input: string | CommitModeContext) {
	if (typeof input === "string") {
		return resolveFromContext({ branch: input });
	}
	return resolveFromContext(input);
}

/** i18n key under namespace `common` (use with `t(commitTranslationKey(...))`). */
export function commitTranslationKey(mode: CommitMode, status: CommitButtonStatus): string {
	if (mode === "merged") {
		return "commit.merged";
	}
	if (mode === "closed") {
		return "commit.closed";
	}

	switch (mode) {
		case "create-pr":
		case "open-pr":
		case "commit-and-push":
		case "push":
		case "fix":
		case "resolve-conflicts":
		case "complete-merge":
		case "merge":
			return `commit.modes.${mode}.${status}`;
		default:
			return "commit.default";
	}
}

export function commitModeClassName(mode: CommitMode) {
	switch (mode) {
		case "fix":
		case "closed":
			return "bg-[var(--workspace-pr-closed-accent)] text-white hover:bg-[var(--workspace-pr-closed-accent)]";
		case "resolve-conflicts":
			return "bg-[var(--workspace-pr-conflicts-accent)] text-white hover:bg-[var(--workspace-pr-conflicts-accent)]";
		case "complete-merge":
			return "bg-emerald-600 text-white hover:bg-emerald-600/90";
		case "merge":
		case "open-pr":
			return "bg-[var(--workspace-pr-open-accent)] text-white hover:bg-[var(--workspace-pr-open-accent)]";
		case "merged":
			return "bg-[var(--workspace-pr-merged-accent)] text-white hover:bg-[var(--workspace-pr-merged-accent)]";
		default:
			return "bg-background text-foreground hover:bg-accent/60";
	}
}
