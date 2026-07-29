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
import type { WorkspaceAgentActivity } from "./use-workspace-agent-states";

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

export function buildWorkspaceRailRecap(input: {
	branch: string;
	activity: WorkspaceAgentActivity | null;
	gitStatus: WorkspaceGitStatusOutput | null;
	prStatus: WorkspacePrStatusOutput | null;
	committedVsBaseCount?: number;
}): WorkspaceRailRecap | null {
	const { activity, gitStatus, prStatus } = input;
	if (!activity && !gitStatus && !prStatus) {
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
		pendingDelegationResultsCount: 0,
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
	onPullRequestMerged?: () => void | Promise<void>;
}): WorkspaceRailRecap | null {
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
	const pullRequestMerged = prStatusQuery.data?.state?.toLowerCase() === "merged";
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
		input.activity.state !== "active" &&
		gitStatusIsClean &&
		prStatusQuery.data != null &&
		prStatusQuery.data.number == null;
	const branchDiffQuery = useWorkspaceGitBranchDiff(
		needsBranchDiff ? root : null,
		RAIL_BRANCH_DIFF_QUERY_OPTIONS,
	);

	return useMemo(
		() => {
			if (
				input.activity?.state !== "active" &&
				(gitStatusQuery.isPending || prStatusQuery.isPending)
			) {
				return null;
			}
			if (needsBranchDiff && branchDiffQuery.isPending) {
				return null;
			}
			return buildWorkspaceRailRecap({
				branch: currentBranch,
				activity: input.activity,
				gitStatus: gitStatusQuery.data ?? null,
				prStatus: prStatusQuery.data ?? null,
				committedVsBaseCount: branchDiffQuery.data?.changes.length ?? 0,
			});
		},
		[
			branchDiffQuery.data?.changes.length,
			branchDiffQuery.isPending,
			currentBranch,
			gitStatusQuery.data,
			gitStatusQuery.isPending,
			input.activity,
			needsBranchDiff,
			prStatusQuery.data,
			prStatusQuery.isPending,
		],
	);
}
