import { useEffect, useMemo, useRef } from "react";
import type {
	WorkspaceGitStatusOutput,
	WorkspacePrStatusOutput,
} from "@dcc/contracts";
import { resolveCommitMode } from "@/features/commit/WorkspaceCommitButton.logic";
import { useWorkspaceGitBranchDiff } from "@/features/inspector/use-workspace-git-branch-diff";
import { useWorkspaceGitStatus } from "@/features/inspector/use-workspace-git-status";
import { useWorkspacePrStatus } from "@/features/inspector/use-workspace-pr-status";
import {
	buildWorkspaceRecap,
	type WorkspaceRecap,
} from "@/features/inspector/workspace-recap";
import {
	isAgentTurnOpen,
	type WorkspaceAgentActivity,
} from "./use-workspace-agent-states";

const RAIL_GIT_QUERY_OPTIONS = {
	staleTime: 20_000,
	refetchInterval: 30_000,
} as const;
const RAIL_PROVIDER_QUERY_OPTIONS = {
	staleTime: 30_000,
	refetchInterval: 60_000,
} as const;
const RAIL_BRANCH_DIFF_QUERY_OPTIONS = {
	staleTime: 60_000,
	refetchInterval: 60_000,
} as const;

export type WorkspaceRailRecap = {
	recap: WorkspaceRecap;
	prTitle: string | null;
};

export type WorkspaceRailPullRequestState = "open" | "draft" | "merged" | "closed";

/** Compact, scannable facts for the row: the PR and the uncommitted diff. */
export type WorkspaceRailMeta = {
	prNumber: number | null;
	prLabel: "PR" | "MR";
	prState: WorkspaceRailPullRequestState | null;
	additions: number;
	deletions: number;
};

export type WorkspaceRailState = {
	currentBranch: string;
	recap: WorkspaceRailRecap | null;
	meta: WorkspaceRailMeta | null;
};

/**
 * Recap messages the row does not repeat: the state line already says the
 * agent is working, and the meta line already shows the PR and the diff.
 */
const RAIL_SILENT_RECAP_KEYS = new Set(["working", "workingClean", "prOpen", "clean"]);

/** The short "next action" key for the row, or null when it would only repeat. */
export function railNextActionKey(messageKey: string): string | null {
	return RAIL_SILENT_RECAP_KEYS.has(messageKey) ? null : `sidebar.nextAction.${messageKey}`;
}

function pullRequestState(
	prStatus: WorkspacePrStatusOutput,
): WorkspaceRailPullRequestState | null {
	const state = prStatus.state?.toLowerCase();
	if (state === "merged") return "merged";
	if (state === "closed") return "closed";
	if (prStatus.isDraft) return "draft";
	return state === "open" || state === "opened" ? "open" : null;
}

export function buildWorkspaceRailMeta(input: {
	branch: string;
	gitStatus: WorkspaceGitStatusOutput | null;
	prStatus: WorkspacePrStatusOutput | null;
}): WorkspaceRailMeta | null {
	const { gitStatus, prStatus } = input;
	const entries = [...(gitStatus?.staged ?? []), ...(gitStatus?.unstaged ?? [])];
	const additions = entries.reduce((sum, entry) => sum + entry.insertions, 0);
	const deletions = entries.reduce((sum, entry) => sum + entry.deletions, 0);
	// Only a PR opened from this exact branch belongs to the task; a fresh
	// worktree can sit on a base commit that an older PR also points at.
	const prHead = prStatus?.headBranch?.trim() ?? "";
	const ownsPullRequest =
		prStatus?.number != null && (prHead.length === 0 || prHead === input.branch.trim());
	const prNumber = ownsPullRequest ? (prStatus?.number ?? null) : null;
	if (prNumber == null && additions === 0 && deletions === 0) {
		return null;
	}
	return {
		prNumber,
		prLabel: prStatus?.provider === "gitlab" ? "MR" : "PR",
		prState: prNumber != null && prStatus ? pullRequestState(prStatus) : null,
		additions,
		deletions,
	};
}

export function isMergedPullRequestForBranch(
	prStatus: WorkspacePrStatusOutput | null | undefined,
	branch: string,
): boolean {
	const currentBranch = branch.trim();
	const pullRequestBranch = prStatus?.headBranch?.trim() ?? "";
	return (
		currentBranch.length > 0 &&
		pullRequestBranch === currentBranch &&
		prStatus?.state?.toLowerCase() === "merged"
	);
}

export function buildWorkspaceRailRecap(input: {
	branch: string;
	activity: WorkspaceAgentActivity | null;
	gitStatus: WorkspaceGitStatusOutput | null;
	prStatus: WorkspacePrStatusOutput | null;
	committedVsBaseCount?: number;
	pendingDelegationResultsCount?: number;
}): WorkspaceRailRecap | null {
	const { activity, gitStatus, prStatus } = input;
	if (!activity && !gitStatus && !prStatus && !input.pendingDelegationResultsCount) {
		return null;
	}

	const entries = [
		...(gitStatus?.staged ?? []),
		...(gitStatus?.unstaged ?? []),
	];
	const changedFilesCount = new Set(entries.map((entry) => entry.path)).size;
	const additions = entries.reduce((sum, entry) => sum + entry.insertions, 0);
	const deletions = entries.reduce((sum, entry) => sum + entry.deletions, 0);
	const commitMode = resolveCommitMode({
		branch: gitStatus?.currentBranch ?? input.branch,
		gitStatus,
		prStatus,
	});
	const recap = buildWorkspaceRecap({
		commitMode,
		// A turn waiting on the user is not "working"; show the git state instead.
		turnRunning: activity?.state === "active",
		changedFilesCount,
		additions,
		deletions,
		aheadOfRemoteCount: gitStatus?.aheadOfRemoteCount ?? 0,
		conflictCount: gitStatus?.conflictCount ?? 0,
		committedVsBaseCount: input.committedVsBaseCount ?? 0,
		prNumber: prStatus?.number ?? null,
		prState: prStatus?.state ?? null,
		requestLabel: prStatus?.provider === "gitlab" ? "MR" : "PR",
		pendingReviewFindingsCount: 0,
		pendingDelegationResultsCount: input.pendingDelegationResultsCount ?? 0,
	});

	// A brand-new idle workspace does not need a redundant "all clean" row.
	if (!activity && recap.messageKey === "clean") {
		return null;
	}

	return {
		recap,
		prTitle: prStatus?.title?.trim() || null,
	};
}

export function useWorkspaceRailRecap(input: {
	workspacePath: string | null;
	branch: string;
	activity: WorkspaceAgentActivity | null;
	enabled: boolean;
	pendingDelegationResultsCount?: number;
	onPullRequestMerged?: () => void | Promise<void>;
}): WorkspaceRailState {
	const root = input.enabled ? input.workspacePath : null;
	const gitStatusQuery = useWorkspaceGitStatus(root, RAIL_GIT_QUERY_OPTIONS);
	const currentBranch = gitStatusQuery.data?.currentBranch ?? input.branch;
	const prStatusQuery = useWorkspacePrStatus(
		root,
		currentBranch,
		null,
		RAIL_PROVIDER_QUERY_OPTIONS,
	);
	const completionAttemptedRef = useRef(false);
	// A fresh protected worktree starts detached at the base branch commit. Forge
	// discovery can legitimately associate that SHA with an older merged PR, but
	// that PR does not belong to the new task. Only an exact head-branch match is
	// authoritative enough to complete a workspace automatically.
	const pullRequestMerged = isMergedPullRequestForBranch(
		prStatusQuery.data,
		currentBranch,
	);
	useEffect(() => {
		if (!pullRequestMerged) {
			completionAttemptedRef.current = false;
			return;
		}
		if (!input.onPullRequestMerged || completionAttemptedRef.current) {
			return;
		}
		completionAttemptedRef.current = true;
		void Promise.resolve()
			.then(() => input.onPullRequestMerged?.())
			.catch(() => {
				completionAttemptedRef.current = false;
			});
	}, [
		input.onPullRequestMerged,
		prStatusQuery.dataUpdatedAt,
		pullRequestMerged,
	]);
	const gitStatusIsClean =
		Boolean(gitStatusQuery.data) &&
		(gitStatusQuery.data?.staged.length ?? 0) === 0 &&
		(gitStatusQuery.data?.unstaged.length ?? 0) === 0 &&
		(gitStatusQuery.data?.aheadOfRemoteCount ?? 0) === 0;
	const needsBranchDiff =
		input.activity != null &&
		!isAgentTurnOpen(input.activity) &&
		gitStatusIsClean &&
		prStatusQuery.data != null &&
		prStatusQuery.data.number == null;
	const branchDiffQuery = useWorkspaceGitBranchDiff(
		needsBranchDiff ? root : null,
		RAIL_BRANCH_DIFF_QUERY_OPTIONS,
	);

	return useMemo(
		() => {
			const meta = buildWorkspaceRailMeta({
				branch: currentBranch,
				gitStatus: gitStatusQuery.data ?? null,
				prStatus: prStatusQuery.data ?? null,
			});
			if (
				!isAgentTurnOpen(input.activity) &&
				(gitStatusQuery.isPending || prStatusQuery.isPending)
			) {
				return { currentBranch, recap: null, meta };
			}
			if (needsBranchDiff && branchDiffQuery.isPending) {
				return { currentBranch, recap: null, meta };
			}
			return {
				currentBranch,
				meta,
				recap: buildWorkspaceRailRecap({
					branch: currentBranch,
					activity: input.activity,
					gitStatus: gitStatusQuery.data ?? null,
					prStatus: prStatusQuery.data ?? null,
					committedVsBaseCount: branchDiffQuery.data?.changes.length ?? 0,
					pendingDelegationResultsCount: input.pendingDelegationResultsCount ?? 0,
				}),
			};
		},
		[
			branchDiffQuery.data?.changes.length,
			branchDiffQuery.isPending,
			currentBranch,
			gitStatusQuery.data,
			gitStatusQuery.isPending,
			input.activity,
			input.pendingDelegationResultsCount,
			needsBranchDiff,
			prStatusQuery.data,
			prStatusQuery.isPending,
		],
	);
}
