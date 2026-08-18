import type {
	ProviderRuntimeConfig,
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
	workspaceGitCommitSuggestion,
	workspacePrStatus,
	workspaceProjectAutomationConfig,
	workspaceRunProjectTasks,
} from "@/lib/workspace-api";
import {
	deriveWorkspaceCommitMessage,
	sanitizeWorkspaceCommitBody,
	sanitizeWorkspaceCommitSubject,
} from "@/features/commit/commit-message";

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

export type MultiWorkspaceDeliveryCommitReview = {
	workspaceId: string;
	subject: string;
	body: string | null;
	stagedFileCount: number;
	stagedFingerprint: string;
	expectedBranch: string | null;
};

export type MultiWorkspaceDeliveryPreview = {
	workspaceId: string;
	name: string;
	action: "commit-and-push" | "push" | "request" | "no-changes" | "blocked";
	commit: MultiWorkspaceDeliveryCommitReview | null;
	message: string | null;
};

export type MultiWorkspaceDeliveryPreparationOptions = {
	providerId?: string | null;
	model?: string | null;
	providerRuntime?: ProviderRuntimeConfig | null;
};

export function selectPreparedMultiWorkspaceMembers(
	members: MultiWorkspaceDeliveryMember[],
	workspaceIds: string[],
) {
	const membersById = new Map<string, MultiWorkspaceDeliveryMember>();
	for (const member of members) {
		if (membersById.has(member.workspaceId)) {
			throw new Error("The multi-project task contains duplicate workspace identities.");
		}
		membersById.set(member.workspaceId, member);
	}
	const requestedIds = new Set(workspaceIds);
	if (requestedIds.size !== workspaceIds.length) {
		throw new Error("The delivery review contains duplicate workspace identities.");
	}
	return workspaceIds.map((workspaceId) => {
		const member = membersById.get(workspaceId);
		if (!member) {
			throw new Error(
				"The multi-project task changed after review. Prepare the delivery again.",
			);
		}
		return member;
	});
}

export type MultiWorkspaceDeliveryDependencies = {
	gitStatus: (workspaceRoot: string) => Promise<WorkspaceGitStatusOutput>;
	commitSuggestion: (
		workspaceRoot: string,
		options?: MultiWorkspaceDeliveryPreparationOptions,
	) => Promise<{
		subject: string;
		body?: string | null;
		stagedFileCount?: number;
		stagedFingerprint?: string;
	}>;
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
	commitPush: (
		workspaceRoot: string,
		message: string,
		body: string | null,
		stagedFingerprint: string,
	) => Promise<void>;
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
	commitSuggestion: async (workspaceRoot, options) => {
		try {
			return await workspaceGitCommitSuggestion({
				workspaceRoot,
				providerId: options?.providerId ?? null,
				model: options?.model ?? null,
				providerRuntime: options?.providerRuntime ?? null,
			});
		} catch {
			const status = await workspaceGitStatus({ workspaceRoot });
			return {
				subject: deriveWorkspaceCommitMessage(status.staged),
				body: null,
				stagedFingerprint: status.stagedFingerprint,
				source: "heuristic-git-staged-fallback",
			};
		}
	},
	branchDiff: (workspaceRoot) => workspaceGitBranchDiff({ workspaceRoot }),
	projectAutomation: (workspaceRoot) =>
		workspaceProjectAutomationConfig({ workspaceRoot }),
	runProjectTasks: (workspaceRoot, taskIds, expectedConfigHash) =>
		workspaceRunProjectTasks({ workspaceRoot, taskIds, expectedConfigHash }),
	stageAll: (workspaceRoot) =>
		workspaceGitStageAll({ workspaceRoot, relativePath: "." }),
	commitPush: (workspaceRoot, message, body, stagedFingerprint) =>
		workspaceGitCommitPush({
			workspaceRoot,
			message,
			body,
			stagedFingerprint,
			forgeLogin: null,
		}),
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

function blockedPreview(
	member: MultiWorkspaceDeliveryMember,
	message: string,
): MultiWorkspaceDeliveryPreview {
	return {
		workspaceId: member.workspaceId,
		name: member.name,
		action: "blocked",
		commit: null,
		message,
	};
}

async function prepareMember(
	member: MultiWorkspaceDeliveryMember,
	options: MultiWorkspaceDeliveryPreparationOptions,
	dependencies: MultiWorkspaceDeliveryDependencies,
): Promise<MultiWorkspaceDeliveryPreview> {
	try {
		let [status, branchDiff] = await Promise.all([
			dependencies.gitStatus(member.workspaceRoot),
			dependencies.branchDiff(member.workspaceRoot),
		]);
		const initialBlock = blockedByLocalGit(status);
		if (initialBlock) return blockedPreview(member, initialBlock);

		const hasLocalWork = hasWorkingChanges(status);
		if (!hasLocalWork) {
			const action = status.aheadOfRemoteCount > 0
				? "push"
				: branchDiff.changes.length > 0
					? "request"
					: "no-changes";
			return {
				workspaceId: member.workspaceId,
				name: member.name,
				action,
				commit: null,
				message: null,
			};
		}

		// Multi-project delivery includes every local change. Staging during
		// preparation mirrors the single-project review and gives the user a
		// fingerprint-bound snapshot before any commit or push can happen.
		await dependencies.stageAll(member.workspaceRoot);
		status = await dependencies.gitStatus(member.workspaceRoot);
		const stagedBlock = blockedByLocalGit(status);
		if (stagedBlock) return blockedPreview(member, stagedBlock);
		if (status.staged.length === 0 || !status.stagedFingerprint) {
			return blockedPreview(
				member,
				"Não foi possível capturar o snapshot staged para revisar o commit.",
			);
		}
		const preparedBranch = status.currentBranch;

		const suggestion = await dependencies.commitSuggestion(member.workspaceRoot, options);
		const verification = await dependencies.gitStatus(member.workspaceRoot);
		const suggestionFingerprint = suggestion.stagedFingerprint?.trim() ?? "";
		if (
			!suggestionFingerprint ||
			verification.currentBranch !== preparedBranch ||
			verification.stagedFingerprint !== suggestionFingerprint ||
			verification.unstaged.length > 0
		) {
			return blockedPreview(
				member,
				"As alterações mudaram durante a preparação. Atualize a revisão antes de entregar.",
			);
		}

		return {
			workspaceId: member.workspaceId,
			name: member.name,
			action: "commit-and-push",
			commit: {
				workspaceId: member.workspaceId,
				subject: sanitizeWorkspaceCommitSubject(suggestion.subject),
				body: sanitizeWorkspaceCommitBody(suggestion.body),
				stagedFileCount: suggestion.stagedFileCount ?? verification.staged.length,
				stagedFingerprint: suggestionFingerprint,
				expectedBranch: preparedBranch,
			},
			message: null,
		};
	} catch (error) {
		return blockedPreview(member, errorMessage(error));
	}
}

export async function prepareMultiWorkspaceDelivery(
	members: MultiWorkspaceDeliveryMember[],
	options: MultiWorkspaceDeliveryPreparationOptions = {},
	dependencies: MultiWorkspaceDeliveryDependencies = defaultDependencies,
) {
	const previews: MultiWorkspaceDeliveryPreview[] = [];
	// Keep provider-backed preparation sequential. Some CLI runtimes own a
	// single sidecar session and should not receive overlapping suggestion turns.
	for (const member of members) {
		previews.push(await prepareMember(member, options, dependencies));
	}
	return previews;
}

async function deliverMember(
	member: MultiWorkspaceDeliveryMember,
	commitReviews: ReadonlyMap<string, MultiWorkspaceDeliveryCommitReview>,
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
				// Re-stage after checks so fixes or edits cannot be silently omitted. The
				// reviewed fingerprint must still match the exact snapshot Git will commit.
				await dependencies.stageAll(member.workspaceRoot);
				const stagedStatus = await dependencies.gitStatus(member.workspaceRoot);
				const review = commitReviews.get(member.workspaceId);
				if (!review) {
					throw new Error(
						"Este projeto possui alterações sem uma mensagem revisada. Atualize a revisão antes de entregar.",
					);
				}
				if (!review.subject.trim()) {
					throw new Error("A mensagem revisada do commit está vazia.");
				}
				if (
					stagedStatus.currentBranch !== review.expectedBranch ||
					stagedStatus.unstaged.length > 0 ||
					!stagedStatus.stagedFingerprint ||
					stagedStatus.stagedFingerprint !== review.stagedFingerprint
				) {
					throw new Error(
						"As alterações staged mudaram após a revisão. Atualize as mensagens antes de entregar.",
					);
				}
				await dependencies.commitPush(
					member.workspaceRoot,
					sanitizeWorkspaceCommitSubject(review.subject),
					sanitizeWorkspaceCommitBody(review.body),
					review.stagedFingerprint,
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
	commitReviews: MultiWorkspaceDeliveryCommitReview[],
	dependencies: MultiWorkspaceDeliveryDependencies = defaultDependencies,
) {
	const results: MultiWorkspaceDeliveryResult[] = [];
	const reviewsByWorkspace = new Map<string, MultiWorkspaceDeliveryCommitReview>();
	const duplicateReviewIds = new Set<string>();
	for (const review of commitReviews) {
		if (reviewsByWorkspace.has(review.workspaceId)) {
			duplicateReviewIds.add(review.workspaceId);
			continue;
		}
		reviewsByWorkspace.set(review.workspaceId, review);
	}
	for (const workspaceId of duplicateReviewIds) reviewsByWorkspace.delete(workspaceId);
	// Keep the bundle order and make partial progress explicit; Git has no transaction
	// spanning repositories, so later failures must not pretend to roll back prior PRs.
	for (const member of members) {
		results.push(await deliverMember(member, reviewsByWorkspace, dependencies));
	}
	return results;
}
