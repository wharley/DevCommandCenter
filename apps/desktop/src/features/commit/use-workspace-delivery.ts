import { useCallback } from "react";
import type { QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { TFunction } from "i18next";
import type { CommitMode } from "./WorkspaceCommitButton.logic";
import {
	deriveWorkspaceCommitMessage,
	sanitizeWorkspaceCommitBody,
	sanitizeWorkspaceCommitSubject,
	type WorkspaceCommitMessageSuggestion,
} from "./commit-message";
import {
	workspaceGitCommit,
	workspaceGitCommitSuggestion,
	workspaceGitCommitPush,
	workspaceGitPush,
	workspaceGitStatus,
	workspaceGitSyncBase,
	workspaceChangeRequestMerge,
	workspaceGitStageAll,
	workspaceChangeRequestViewWeb,
	workspaceProjectAutomationConfig,
	workspaceRunProjectTasks,
	workspaceChangeRequestCreate,
} from "@/lib/workspace-api";
import type { ProviderRuntimeConfig, WorkspaceChangeRequestCreateInput } from "@dcc/contracts";
import {
	WORKSPACE_GIT_STATUS_QUERY_KEY,
} from "@/features/inspector/use-workspace-git-status";
import { WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY } from "@/features/inspector/use-workspace-git-branch-diff";
import { WORKSPACE_PR_STATUS_QUERY_KEY } from "@/features/inspector/use-workspace-pr-status";
import { WORKSPACE_FORGE_CONTEXT_QUERY_KEY } from "@/features/inspector/use-workspace-forge-context";
import { WORKSPACE_PIPELINE_QUERY_KEY } from "@/features/inspector/use-workspace-pipeline";
import { WORKSPACE_DELIVERY_FAILURE_QUERY_KEY } from "@/features/inspector/use-workspace-delivery-failure";
import { WORKSPACE_REVIEW_STATE_QUERY_KEY } from "@/features/inspector/use-workspace-review-state";
import {
	setWorkspaceDeliveryBusy,
	useWorkspaceDeliveryBusy,
} from "./workspace-delivery-busy";

type WorkspaceDeliveryControllerOptions = {
	workspaceRoot: string | null;
	forgeLogin: string | null;
	baseBranch: string | null;
	requestLabel?: "PR" | "MR";
	stagedCount: number;
	hasLocalChanges: boolean;
	multiProject?: boolean;
	onReview: () => void;
	onRequestSyncBase?: () => void;
	onRequestMerge?: () => void;
	onCompleteWorkspace?: () => Promise<void> | void;
	onOpenMultiProject?: () => void;
	onCreateRequest?: (draft: boolean) => void;
	queryClient: QueryClient;
	t: TFunction<"common">;
};

export type WorkspaceCommitSuggestionOptions = {
	providerId?: string | null;
	model?: string | null;
	providerRuntime?: ProviderRuntimeConfig | null;
};

export type WorkspaceDeliveryCreateRequestInput = WorkspaceChangeRequestCreateInput & {
	includeLocalChanges: boolean;
};

function errorMessage(error: unknown) {
	return error instanceof Error ? error.message : String(error);
}

async function commitMessageForStagedChanges(
	workspaceRoot: string,
) {
	try {
		// Automatic delivery never invokes a provider. Provider output is allowed
		// only through prepareWorkspaceCommitMessage, which opens a human preview.
		const suggestion = await workspaceGitCommitSuggestion({ workspaceRoot });
		return {
			...suggestion,
			subject: sanitizeWorkspaceCommitSubject(suggestion.subject),
			body: sanitizeWorkspaceCommitBody(suggestion.body),
		};
	} catch {
		// Keep an offline/older-runtime fallback. It is still derived only from
		// the freshly-read staged Git status and never from task/chat context.
		const status = await workspaceGitStatus({ workspaceRoot });
		if (!status.stagedFingerprint) {
			throw new Error("Unable to capture the staged Git snapshot; refresh and try again");
		}
		return {
			subject: deriveWorkspaceCommitMessage(status.staged),
			body: null,
			stagedFingerprint: status.stagedFingerprint,
			source: "heuristic-git-staged-fallback",
		};
	}
}

export async function prepareWorkspaceCommitMessage(
	workspaceRoot: string,
	options?: WorkspaceCommitSuggestionOptions,
): Promise<WorkspaceCommitMessageSuggestion> {
	try {
		const suggestion = await workspaceGitCommitSuggestion({
			workspaceRoot,
			providerId: options?.providerId ?? null,
			model: options?.model ?? null,
			providerRuntime: options?.providerRuntime ?? null,
		});
		return {
			...suggestion,
			subject: sanitizeWorkspaceCommitSubject(suggestion.subject),
			body: sanitizeWorkspaceCommitBody(suggestion.body),
		};
	} catch {
		const status = await workspaceGitStatus({ workspaceRoot });
		if (!status.stagedFingerprint) {
			throw new Error("Unable to capture the staged Git snapshot; refresh and try again");
		}
		return {
			subject: deriveWorkspaceCommitMessage(status.staged),
			body: null,
			stagedFileCount: status.staged.length,
			stagedFingerprint: status.stagedFingerprint,
			source: "git-staged-fallback",
		};
	}
}

export function useWorkspaceDelivery({
	workspaceRoot,
	forgeLogin,
	baseBranch,
	requestLabel = "PR",
	stagedCount,
	hasLocalChanges,
	multiProject = false,
	onReview,
	onRequestSyncBase,
	onRequestMerge,
	onCompleteWorkspace,
	onOpenMultiProject,
	onCreateRequest,
	queryClient,
	t,
}: WorkspaceDeliveryControllerOptions) {
	const busy = useWorkspaceDeliveryBusy(workspaceRoot);
	const invalidateGitState = useCallback(async () => {
		const root = workspaceRoot?.trim();
		if (!root) return;
		await Promise.all([
			queryClient.invalidateQueries({ queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root] }),
			queryClient.invalidateQueries({ queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, root] }),
			queryClient.invalidateQueries({ queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root] }),
			queryClient.invalidateQueries({ queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY, root] }),
			queryClient.invalidateQueries({ queryKey: [WORKSPACE_PIPELINE_QUERY_KEY, root] }),
			queryClient.invalidateQueries({ queryKey: [WORKSPACE_DELIVERY_FAILURE_QUERY_KEY, root] }),
			queryClient.invalidateQueries({ queryKey: [WORKSPACE_REVIEW_STATE_QUERY_KEY, root] }),
		]);
	}, [queryClient, workspaceRoot]);

	const runBeforePushChecks = useCallback(async (root: string) => {
		const config = await queryClient.fetchQuery({
			queryKey: ["workspaceProjectAutomationConfig", root],
			queryFn: () => workspaceProjectAutomationConfig({ workspaceRoot: root }),
			staleTime: 0,
		});
		if (config.beforePush.length === 0) return true;
		const output = await workspaceRunProjectTasks({
			workspaceRoot: root,
			taskIds: config.beforePush,
			expectedConfigHash: config.configHash,
		});
		if (output.changedFiles) await invalidateGitState();
		return output.report.status === "passed";
	}, [invalidateGitState, queryClient]);

	const run = useCallback(async (
		mode: CommitMode | "commit" | "sync-base",
		commitMessage?: string,
		commitBody?: string | null,
		stagedFingerprint?: string,
	) => {
		if (multiProject) {
			onOpenMultiProject?.();
			return;
		}
		if (busy) return;
		const root = workspaceRoot?.trim();
		if (!root) {
			toast.error(t("composer.executionDock.actions.unavailable"));
			return;
		}

		if (mode === "create-pr") {
			onCreateRequest?.(false);
			return;
		}
		if (mode === "sync-base") {
			onRequestSyncBase?.();
			return;
		}
		if (mode === "merge") {
			onRequestMerge?.();
			return;
		}
		if (mode === "fix" || mode === "resolve-conflicts" || mode === "complete-merge") {
			onReview();
			return;
		}
		if (mode === "merged") return;

		setWorkspaceDeliveryBusy(root, true);
		try {
			switch (mode) {
				case "commit":
					if (!hasLocalChanges) throw new Error(t("composer.executionDock.actions.noLocalChanges"));
					if (stagedCount === 0) {
						await workspaceGitStageAll({ workspaceRoot: root, relativePath: "." });
					}
					const commitSuggestion = commitMessage
						? {
							subject: sanitizeWorkspaceCommitSubject(commitMessage),
							body: sanitizeWorkspaceCommitBody(commitBody),
							stagedFingerprint,
						}
						: await commitMessageForStagedChanges(root);
					if (!commitSuggestion.stagedFingerprint) throw new Error("Staged preview expired; refresh and try again");
					await workspaceGitCommit({
						workspaceRoot: root,
						message: commitSuggestion.subject,
						body: commitSuggestion.body,
						stagedFingerprint: commitSuggestion.stagedFingerprint,
					});
					toast.success(t("composer.executionDock.actions.committed"));
					break;
				case "push":
					if (!(await runBeforePushChecks(root))) {
						throw new Error(t("composer.executionDock.actions.beforePushBlocked"));
					}
					await workspaceGitPush({ workspaceRoot: root, forgeLogin });
					toast.success(t("composer.executionDock.actions.pushed"));
					break;
				case "commit-and-push":
				default:
					if (!(await runBeforePushChecks(root))) {
						throw new Error(t("composer.executionDock.actions.beforePushBlocked"));
					}
					if (stagedCount === 0) {
						await workspaceGitStageAll({ workspaceRoot: root, relativePath: "." });
					}
					const pushSuggestion = commitMessage
						? {
							subject: sanitizeWorkspaceCommitSubject(commitMessage),
							body: sanitizeWorkspaceCommitBody(commitBody),
							stagedFingerprint,
						}
						: await commitMessageForStagedChanges(root);
					if (!pushSuggestion.stagedFingerprint) throw new Error("Staged preview expired; refresh and try again");
					await workspaceGitCommitPush({
						workspaceRoot: root,
						message: pushSuggestion.subject,
						body: pushSuggestion.body,
						stagedFingerprint: pushSuggestion.stagedFingerprint,
						forgeLogin,
					});
					toast.success(t("composer.executionDock.actions.committedAndPushed"));
					break;
				case "open-pr":
				case "closed":
					await workspaceChangeRequestViewWeb({ workspaceRoot: root, forgeLogin });
					toast.success(t("composer.executionDock.actions.opened"));
					break;
			}
			await invalidateGitState();
		} catch (error) {
			toast.error(t("composer.executionDock.actions.failed"), {
				description: errorMessage(error),
			});
			throw error;
		} finally {
			setWorkspaceDeliveryBusy(root, false);
		}
	}, [
		busy,
		forgeLogin,
		hasLocalChanges,
		invalidateGitState,
		multiProject,
		onCreateRequest,
		onOpenMultiProject,
		onRequestSyncBase,
		onRequestMerge,
		onReview,
		runBeforePushChecks,
		stagedCount,
		t,
		// The optional preview text is user-editable; sanitize again at the
		// boundary before it reaches the Git command.
		workspaceRoot,
	]);

	const syncBase = useCallback(async () => {
		const root = workspaceRoot?.trim();
		if (!root || busy) return;

		setWorkspaceDeliveryBusy(root, true);
		const loadingToast = toast.loading(t("inspector.gitConfirmation.syncLoading"));
		try {
			const result = await workspaceGitSyncBase({
				workspaceRoot: root,
				baseBranch,
				forgeLogin,
			});
			const baseRef = `${result.remote}/${result.baseBranch}`;
			if (result.updated) {
				toast.success(t("inspector.gitConfirmation.syncSuccess", { baseRef }), { id: loadingToast });
			} else {
				toast.info(t("inspector.gitConfirmation.syncAlreadyCurrent", { baseRef }), { id: loadingToast });
			}
			await invalidateGitState();
		} catch (error) {
			toast.error(t("inspector.gitConfirmation.syncFailed", { message: errorMessage(error) }), {
				id: loadingToast,
			});
		} finally {
			setWorkspaceDeliveryBusy(root, false);
		}
	}, [baseBranch, busy, forgeLogin, invalidateGitState, t, workspaceRoot]);

	const merge = useCallback(async () => {
		const root = workspaceRoot?.trim();
		if (!root || busy) return;

		setWorkspaceDeliveryBusy(root, true);
		const loadingToast = toast.loading(t("inspector.gitConfirmation.mergeLoading", { requestLabel }));
		try {
			await workspaceChangeRequestMerge({ workspaceRoot: root, forgeLogin });
			toast.success(t("inspector.gitConfirmation.mergeSuccess", { requestLabel }), {
				id: loadingToast,
			});
			try {
				await onCompleteWorkspace?.();
			} catch (completionError) {
				toast.error(
					t("sidebar.completeWorkspaceError", { message: errorMessage(completionError) }),
				);
			}
			await invalidateGitState();
		} catch (error) {
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_DELIVERY_FAILURE_QUERY_KEY, root],
			});
			toast.error(t("inspector.gitConfirmation.mergeFailed", {
				requestLabel,
				message: errorMessage(error),
			}), { id: loadingToast });
		} finally {
			setWorkspaceDeliveryBusy(root, false);
		}
	}, [busy, forgeLogin, invalidateGitState, onCompleteWorkspace, queryClient, requestLabel, t, workspaceRoot]);

	const createRequest = useCallback(async (input: WorkspaceDeliveryCreateRequestInput) => {
		const root = workspaceRoot?.trim();
		if (!root || busy) return;
		setWorkspaceDeliveryBusy(root, true);
		try {
			if (input.includeLocalChanges && hasLocalChanges) {
				if (!(await runBeforePushChecks(root))) {
					throw new Error(t("composer.executionDock.actions.beforePushBlocked"));
				}
				if (stagedCount === 0) {
					await workspaceGitStageAll({ workspaceRoot: root, relativePath: "." });
				}
				const suggestion = await commitMessageForStagedChanges(root);
				await workspaceGitCommitPush({
					workspaceRoot: root,
					message: suggestion.subject,
					body: suggestion.body,
					stagedFingerprint: suggestion.stagedFingerprint,
					forgeLogin,
				});
			}
			await workspaceChangeRequestCreate({
				workspaceRoot: input.workspaceRoot,
				forgeLogin: input.forgeLogin,
				title: input.title,
				body: input.body,
				draft: input.draft,
			});
			toast.success(t("composer.executionDock.createRequest.created", { requestLabel }));
			await invalidateGitState();
		} catch (error) {
			toast.error(t("composer.executionDock.actions.failed"), { description: errorMessage(error) });
			throw error;
		} finally {
			setWorkspaceDeliveryBusy(root, false);
		}
	}, [
		busy,
		forgeLogin,
		hasLocalChanges,
		invalidateGitState,
		runBeforePushChecks,
		requestLabel,
		stagedCount,
		t,
		workspaceRoot,
	]);

	return { busy, run, syncBase, merge, createRequest, invalidateGitState };
}
