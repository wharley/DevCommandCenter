import type {
	WorkspaceGitBranchDiffOutput,
	WorkspaceGitStatusOutput,
	WorkspacePrStatusOutput,
	WorkspaceProjectAutomationConfigOutput,
	WorkspaceRunProjectTasksOutput,
} from "@dcc/contracts";
import {
	workspaceChangeRequestCreate,
	workspaceGitBranchDiff,
	workspaceGitCommitPush,
	workspaceGitPush,
	workspaceGitStageAll,
	workspaceGitStatus,
	workspacePrStatus,
	workspaceProjectAutomationConfig,
	workspaceRunProjectTasks,
} from "@/lib/workspace-api";

export type MultiWorkspaceDeliveryMember = {
	workspaceId: string;
	name: string;
	workspaceRoot: string;
};

export type MultiWorkspaceDeliveryResult = {
	workspaceId: string;
	name: string;
	status: "delivered" | "skipped" | "failed";
	action:
		| "created-request"
		| "updated-request"
		| "existing-request"
		| "no-changes"
		| "blocked";
	message: string;
	requestUrl: string | null;
};

export type MultiWorkspaceDeliveryDependencies = {
	gitStatus: (workspaceRoot: string) => Promise<WorkspaceGitStatusOutput>;
	branchDiff: (workspaceRoot: string) => Promise<WorkspaceGitBranchDiffOutput>;
	projectAutomation: (
		workspaceRoot: string,
	) => Promise<WorkspaceProjectAutomationConfigOutput>;
	runProjectTasks: (
		workspaceRoot: string,
		taskIds: string[],
		expectedConfigHash: string | null,
	) => Promise<WorkspaceRunProjectTasksOutput>;
	stageAll: (workspaceRoot: string) => Promise<void>;
	commitPush: (workspaceRoot: string, message: string) => Promise<void>;
	push: (workspaceRoot: string) => Promise<void>;
	requestStatus: (
		workspaceRoot: string,
		branch: string | null,
	) => Promise<WorkspacePrStatusOutput>;
	createRequest: (workspaceRoot: string) => Promise<void>;
};

export function resolveMultiWorkspaceDeliveryState({
	gitStatus,
	branchDiff,
	requestState,
}: {
	gitStatus: WorkspaceGitStatusOutput;
	branchDiff: WorkspaceGitBranchDiffOutput;
	requestState: string | null;
}) {
	const hasWorkingChanges =
		gitStatus.staged.length > 0 || gitStatus.unstaged.length > 0;
	const hasUnpushedCommits = gitStatus.aheadOfRemoteCount > 0;
	const hasBranchDiff = branchDiff.changes.length > 0;
	const normalizedRequestState = requestState?.toLowerCase() ?? null;
	const requestFinished =
		normalizedRequestState === "merged" || normalizedRequestState === "closed";
	const needsDelivery = requestFinished
		? false
		: normalizedRequestState === "open"
			? hasWorkingChanges || hasUnpushedCommits
			: hasWorkingChanges || hasUnpushedCommits || hasBranchDiff;

	return {
		hasChanges: hasWorkingChanges || hasUnpushedCommits || hasBranchDiff,
		needsDelivery,
	};
}

const defaultDependencies: MultiWorkspaceDeliveryDependencies = {
	gitStatus: (workspaceRoot) => workspaceGitStatus({ workspaceRoot }),
	branchDiff: (workspaceRoot) => workspaceGitBranchDiff({ workspaceRoot }),
	projectAutomation: (workspaceRoot) =>
		workspaceProjectAutomationConfig({ workspaceRoot }),
	runProjectTasks: (workspaceRoot, taskIds, expectedConfigHash) =>
		workspaceRunProjectTasks({ workspaceRoot, taskIds, expectedConfigHash }),
	stageAll: (workspaceRoot) =>
		workspaceGitStageAll({ workspaceRoot, relativePath: "." }),
	commitPush: (workspaceRoot, message) =>
		workspaceGitCommitPush({ workspaceRoot, message, forgeLogin: null }),
	push: (workspaceRoot) => workspaceGitPush({ workspaceRoot, forgeLogin: null }),
	requestStatus: (workspaceRoot, branch) =>
		workspacePrStatus({ workspaceRoot, branch, forgeLogin: null }),
	createRequest: (workspaceRoot) =>
		workspaceChangeRequestCreate({
			workspaceRoot,
			forgeLogin: null,
			title: null,
			body: null,
			draft: false,
		}),
};

function errorMessage(error: unknown) {
	if (error instanceof Error) return error.message;
	if (typeof error === "string") return error;
	return String(error);
}

function hasWorkingChanges(status: WorkspaceGitStatusOutput) {
	return status.staged.length > 0 || status.unstaged.length > 0;
}

function blockedByLocalGit(status: WorkspaceGitStatusOutput) {
	if (status.mergeInProgress) {
		return "Há um merge em andamento. Resolva-o no Inspector antes de entregar.";
	}
	if (status.conflictCount > 0) {
		return "Há conflitos locais. Resolva-os no Inspector antes de entregar.";
	}
	return null;
}

async function deliverMember(
	member: MultiWorkspaceDeliveryMember,
	dependencies: MultiWorkspaceDeliveryDependencies,
): Promise<MultiWorkspaceDeliveryResult> {
	let published = false;

	try {
		let [status, branchDiff] = await Promise.all([
			dependencies.gitStatus(member.workspaceRoot),
			dependencies.branchDiff(member.workspaceRoot),
		]);
		const initialBlock = blockedByLocalGit(status);
		if (initialBlock) {
			return {
				workspaceId: member.workspaceId,
				name: member.name,
				status: "failed",
				action: "blocked",
				message: initialBlock,
				requestUrl: null,
			};
		}

		const hasLocalWork = hasWorkingChanges(status);
		const hasUnpushedCommits = status.aheadOfRemoteCount > 0;
		if (!hasLocalWork && !hasUnpushedCommits && branchDiff.changes.length === 0) {
			return {
				workspaceId: member.workspaceId,
				name: member.name,
				status: "skipped",
				action: "no-changes",
				message: "Sem alterações; nenhuma branch ou PR foi publicado.",
				requestUrl: null,
			};
		}

		if (hasLocalWork || hasUnpushedCommits) {
			const automation = await dependencies.projectAutomation(member.workspaceRoot);
			if (automation.beforePush.length > 0) {
				const checks = await dependencies.runProjectTasks(
					member.workspaceRoot,
					automation.beforePush,
					automation.configHash,
				);
				if (checks.report.status !== "passed") {
					return {
						workspaceId: member.workspaceId,
						name: member.name,
						status: "failed",
						action: "blocked",
						message: "Os checks obrigatórios de beforePush falharam.",
						requestUrl: null,
					};
				}
			}

			// Checks may apply fixes, so re-read Git state before staging and publishing.
			status = await dependencies.gitStatus(member.workspaceRoot);
			const postCheckBlock = blockedByLocalGit(status);
			if (postCheckBlock) {
				return {
					workspaceId: member.workspaceId,
					name: member.name,
					status: "failed",
					action: "blocked",
					message: postCheckBlock,
					requestUrl: null,
				};
			}

			if (hasWorkingChanges(status)) {
				// The coordinated action is explicitly an "all changed projects" delivery.
				// Stage the complete worktree so no local file is silently left outside its PR.
				await dependencies.stageAll(member.workspaceRoot);
				await dependencies.commitPush(
					member.workspaceRoot,
					`chore: checkpoint for ${member.name}`,
				);
				published = true;
			} else if (status.aheadOfRemoteCount > 0) {
				await dependencies.push(member.workspaceRoot);
				published = true;
			}
		}

		branchDiff = await dependencies.branchDiff(member.workspaceRoot);
		if (branchDiff.changes.length === 0) {
			return {
				workspaceId: member.workspaceId,
				name: member.name,
				status: "skipped",
				action: "no-changes",
				message: published
					? "A branch foi publicada, mas não há diferença contra a branch base para abrir PR."
					: "Sem diferença contra a branch base para abrir PR.",
				requestUrl: null,
			};
		}

		const currentStatus = await dependencies.gitStatus(member.workspaceRoot);
		const request = await dependencies.requestStatus(
			member.workspaceRoot,
			currentStatus.currentBranch,
		);
		const requestState = request.state?.toLowerCase() ?? null;
		if (requestState === "open") {
			return {
				workspaceId: member.workspaceId,
				name: member.name,
				status: "delivered",
				action: published ? "updated-request" : "existing-request",
				message: published
					? "Commit/push concluído e o PR existente foi atualizado."
					: "O PR deste projeto já estava aberto.",
				requestUrl: request.url,
			};
		}
		if (requestState === "merged" || requestState === "closed") {
			return {
				workspaceId: member.workspaceId,
				name: member.name,
				status: "failed",
				action: "blocked",
				message: `O PR desta branch já está ${requestState === "merged" ? "mesclado" : "fechado"}; crie um novo workspace para continuar.`,
				requestUrl: request.url,
			};
		}

		await dependencies.createRequest(member.workspaceRoot);
		let requestUrl: string | null = null;
		try {
			const createdRequest = await dependencies.requestStatus(
				member.workspaceRoot,
				currentStatus.currentBranch,
			);
			requestUrl = createdRequest.url;
		} catch {
			// Creation already succeeded. A follow-up lookup must not turn it into a failure.
		}
		return {
			workspaceId: member.workspaceId,
			name: member.name,
			status: "delivered",
			action: "created-request",
			message: "Commit/push concluído e PR criado.",
			requestUrl,
		};
	} catch (error) {
		return {
			workspaceId: member.workspaceId,
			name: member.name,
			status: "failed",
			action: "blocked",
			message: published
				? `A branch foi publicada, mas o PR falhou: ${errorMessage(error)}`
				: errorMessage(error),
			requestUrl: null,
		};
	}
}

export async function deliverMultiWorkspace(
	members: MultiWorkspaceDeliveryMember[],
	dependencies: MultiWorkspaceDeliveryDependencies = defaultDependencies,
) {
	const results: MultiWorkspaceDeliveryResult[] = [];
	// Keep the bundle order and make partial progress explicit; Git has no transaction
	// spanning repositories, so later failures must not pretend to roll back prior PRs.
	for (const member of members) {
		results.push(await deliverMember(member, dependencies));
	}
	return results;
}
