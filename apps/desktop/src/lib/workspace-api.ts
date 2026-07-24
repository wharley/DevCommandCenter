import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WORKSPACE_METHODS } from "@dcc/contracts";
import type {
	CodeRabbitReviewStreamEvent,
	CreateWorkspaceForRepoInput,
	CreateWorkspaceForRepoOutput,
	CreateWorkspaceBundleForReposInput,
	CreateWorkspaceBundleForReposOutput,
	CreateWorkspaceFromSourceUrlInput,
	CreateWorkspaceFromUrlInput,
	CreateWorkspaceFromUrlOutput,
	CompileMissionSpecContextInput,
	CompileMissionSpecContextOutput,
	WorkspaceCodeRabbitCliStatusInput,
	WorkspaceCodeRabbitCliStatusOutput,
	WorkspaceCodeRabbitDoctorInput,
	WorkspaceCodeRabbitDoctorOutput,
	WorkspaceCodeRabbitFingerprintInput,
	CodeRabbitDiffFingerprint,
	WorkspaceCodeRabbitReviewInput,
	WorkspaceCodeRabbitReviewJobInput,
	WorkspaceCodeRabbitReviewJobSnapshot,
	WorkspaceCodeRabbitReviewOutput,
	WorkspaceCodeRabbitReviewStartOutput,
	WorkspaceCodeRabbitReviewHistoryInput,
	WorkspaceCodeRabbitReviewHistoryOutput,
	WorkspaceCodeRabbitSaveReviewInput,
	WorkspaceCodeRabbitStoredReviewInput,
	WorkspaceCodeRabbitStoredReviewOutput,
	ForgeCliAccountsInput,
	ForgeCliAccountsOutput,
	ForgeCliHostsInput,
	ForgeCliHostsOutput,
	ForgeCliSelectLoginInput,
	ForgeCliStatusInput,
	ForgeCliStatusOutput,
	GithubCliStatusInput,
	GithubCliStatusOutput,
	ListChildDirectoriesInput,
	ListChildDirectoriesOutput,
	ListGitTrackedFilesInput,
	ListGitTrackedFilesOutput,
	ReadWorkspaceFileInput,
	ReadWorkspaceFileOutput,
	ResolveWorkspaceSourceUrlInput,
	WriteWorkspaceFileInput,
	WriteWorkspaceFileOutput,
	SearchWorkspaceInput,
	SearchWorkspaceOutput,
	ListMissionSpecsInput,
	ListMissionSpecsOutput,
	ListLocalBranchesInput,
	ListLocalBranchesOutput,
	ListRepositoriesOutput,
	ListWorkspaceBundlesOutput,
	ListWorkspacesOutput,
	MissionSpecContextStatusInput,
	MissionSpecContextStatusOutput,
	WorkspaceGitBranchDiffInput,
	WorkspaceGitBranchDiffOutput,
	WorkspaceGitFilePreviewInput,
	WorkspaceGitFilePreviewContentOutput,
	WorkspaceGitCommitPushInput,
	WorkspaceGitCompleteMergeInput,
	WorkspaceGitCompleteMergeOutput,
	WorkspaceGitAcceptConflictInput,
	WorkspaceGitConflictStateInput,
	WorkspaceGitConflictStateOutput,
	WorkspaceGitMarkConflictResolvedInput,
	WorkspaceGitPathInput,
	WorkspaceGitPushInput,
	WorkspaceGitStatusInput,
	WorkspaceGitStatusOutput,
	WorkspaceGitSyncBaseInput,
	WorkspaceGitSyncBaseOutput,
	WorkspaceGitValidationConfigOutput,
	WorkspaceProjectAutomationConfigOutput,
	WorkspaceRunProjectTasksInput,
	WorkspaceRunProjectTasksOutput,
	WorkspaceSaveProjectAutomationInput,
	WorkspaceApplyDelegationWorktreeInput,
	WorkspaceApplyDelegationWorktreeOutput,
	WorkspacePrepareDelegationWorktreeInput,
	WorkspacePrepareDelegationWorktreeOutput,
	WorkspaceRemoveDelegationWorktreeInput,
	SaveMissionValidationInput,
	SaveMissionValidationOutput,
	WorkspaceForgeContextInput,
	WorkspaceForgeContextOutput,
	WorkspaceContinueFromBaseBranchInput,
	WorkspaceContinueFromBaseBranchOutput,
	WorkspaceDeliveryFailureInput,
	WorkspaceDeliveryFailureOutput,
	WorkspaceDeliveryRecoveryInput,
	WorkspaceDeliveryRecoveryOutput,
	WorkspacePrReviewCommentsInput,
	WorkspacePrReviewCommentsOutput,
	WorkspacePrStatusInput,
	WorkspacePrStatusOutput,
	WorkspacePipelineJobInput,
	WorkspacePipelineJobLogOutput,
	WorkspacePipelineStatusInput,
	WorkspacePipelineStatusOutput,
	WorkspaceReviewStateInput,
	WorkspaceReviewStateOutput,
	WorkspaceRunSetupInput,
	WorkspaceRunSetupOutput,
	WorkspaceBundleStateOutput,
	WorkspaceSourceUrlResolution,
} from "@dcc/contracts";

export const CODERABBIT_REVIEW_EVENT_NAME = "dcc/coderabbit/review/event";

export function createWorkspaceForRepo(input: CreateWorkspaceForRepoInput) {
	return invoke<CreateWorkspaceForRepoOutput>(WORKSPACE_METHODS.createWorkspaceForRepo, {
		input,
	});
}

export function resolveWorkspaceSourceUrl(input: ResolveWorkspaceSourceUrlInput) {
	return invoke<WorkspaceSourceUrlResolution>(
		WORKSPACE_METHODS.resolveWorkspaceSourceUrl,
		{ input },
	);
}

export function createWorkspaceFromSourceUrl(input: CreateWorkspaceFromSourceUrlInput) {
	return invoke<CreateWorkspaceForRepoOutput>(
		WORKSPACE_METHODS.createWorkspaceFromSourceUrl,
		{ input },
	);
}

export function createWorkspaceBundleForRepos(
	input: CreateWorkspaceBundleForReposInput,
) {
	return invoke<CreateWorkspaceBundleForReposOutput>(
		WORKSPACE_METHODS.createWorkspaceBundleForRepos,
		{ input },
	);
}

export function archiveWorkspace(workspaceId: string) {
	return invoke<void>(WORKSPACE_METHODS.archiveWorkspace, { input: { workspaceId } });
}

export function restoreWorkspace(workspaceId: string) {
	return invoke<void>(WORKSPACE_METHODS.restoreWorkspace, { input: { workspaceId } });
}

export function deleteWorkspace(workspaceId: string) {
	return invoke<void>(WORKSPACE_METHODS.deleteWorkspace, { input: { workspaceId } });
}

export function deleteRepository(repositoryId: string) {
	return invoke<void>(WORKSPACE_METHODS.deleteRepository, { input: { repositoryId } });
}

export function createWorkspaceFromUrl(input: CreateWorkspaceFromUrlInput) {
	return invoke<CreateWorkspaceFromUrlOutput>(WORKSPACE_METHODS.createWorkspaceFromUrl, {
		input,
	});
}

export function workspaceGithubCliStatus(input: GithubCliStatusInput) {
	return invoke<GithubCliStatusOutput>(WORKSPACE_METHODS.workspaceGithubCliStatus, {
		input,
	});
}

export function workspaceForgeCliStatus(input: ForgeCliStatusInput) {
	return invoke<ForgeCliStatusOutput>(WORKSPACE_METHODS.workspaceForgeCliStatus, {
		input,
	});
}

export function workspaceForgeCliAccounts(input: ForgeCliAccountsInput) {
	return invoke<ForgeCliAccountsOutput>(WORKSPACE_METHODS.workspaceForgeCliAccounts, {
		input,
	});
}

export function workspaceForgeCliHosts(input: ForgeCliHostsInput) {
	return invoke<ForgeCliHostsOutput>(WORKSPACE_METHODS.workspaceForgeCliHosts, {
		input,
	});
}

export function workspaceForgeCliSelectLogin(input: ForgeCliSelectLoginInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceForgeCliSelectLogin, {
		input,
	});
}

export function workspaceBackfillForgeRepoBindings() {
	return invoke<number>(WORKSPACE_METHODS.workspaceBackfillForgeRepoBindings);
}

export function workspaceRetryRepositoryForgeBinding(repositoryId: string) {
	return invoke<string | null>(WORKSPACE_METHODS.workspaceRetryRepositoryForgeBinding, {
		input: { repositoryId },
	});
}

export function workspaceForgeContext(input: WorkspaceForgeContextInput) {
	return invoke<WorkspaceForgeContextOutput>(WORKSPACE_METHODS.workspaceForgeContext, {
		input,
	});
}

export function workspaceCodeRabbitCliStatus(input: WorkspaceCodeRabbitCliStatusInput) {
	return invoke<WorkspaceCodeRabbitCliStatusOutput>(
		WORKSPACE_METHODS.workspaceCoderabbitCliStatus,
		{ input },
	);
}

export function workspaceCodeRabbitDoctor(input: WorkspaceCodeRabbitDoctorInput) {
	return invoke<WorkspaceCodeRabbitDoctorOutput>(
		WORKSPACE_METHODS.workspaceCoderabbitDoctor,
		{ input },
	);
}

export function workspaceCodeRabbitDiffFingerprint(
	input: WorkspaceCodeRabbitFingerprintInput,
) {
	return invoke<CodeRabbitDiffFingerprint>(
		WORKSPACE_METHODS.workspaceCoderabbitDiffFingerprint,
		{ input },
	);
}

export function workspaceCodeRabbitReview(input: WorkspaceCodeRabbitReviewInput) {
	return invoke<WorkspaceCodeRabbitReviewOutput>(
		WORKSPACE_METHODS.workspaceCoderabbitReview,
		{ input },
	);
}

export function workspaceCodeRabbitReviewStart(input: WorkspaceCodeRabbitReviewInput) {
	return invoke<WorkspaceCodeRabbitReviewStartOutput>(
		WORKSPACE_METHODS.workspaceCoderabbitReviewStart,
		{ input },
	);
}

export function workspaceCodeRabbitReviewJob(input: WorkspaceCodeRabbitReviewJobInput) {
	return invoke<WorkspaceCodeRabbitReviewJobSnapshot>(
		WORKSPACE_METHODS.workspaceCoderabbitReviewJob,
		{ input },
	);
}

export function workspaceCodeRabbitReviewCancel(input: WorkspaceCodeRabbitReviewJobInput) {
	return invoke<WorkspaceCodeRabbitReviewJobSnapshot>(
		WORKSPACE_METHODS.workspaceCoderabbitReviewCancel,
		{ input },
	);
}

export function workspaceCodeRabbitReviewLoad(input: WorkspaceCodeRabbitStoredReviewInput) {
	return invoke<WorkspaceCodeRabbitStoredReviewOutput>(
		WORKSPACE_METHODS.workspaceCoderabbitReviewLoad,
		{ input },
	);
}

export function workspaceCodeRabbitReviewSave(input: WorkspaceCodeRabbitSaveReviewInput) {
	return invoke<WorkspaceCodeRabbitStoredReviewOutput>(
		WORKSPACE_METHODS.workspaceCoderabbitReviewSave,
		{ input },
	);
}

export function workspaceCodeRabbitReviewHistory(
	input: WorkspaceCodeRabbitReviewHistoryInput,
) {
	return invoke<WorkspaceCodeRabbitReviewHistoryOutput>(
		WORKSPACE_METHODS.workspaceCoderabbitReviewHistory,
		{ input },
	);
}

export function workspaceCodeRabbitReviewClear(input: WorkspaceCodeRabbitStoredReviewInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceCoderabbitReviewClear, { input });
}

export async function listenCodeRabbitReviewEvents(
	handler: (event: CodeRabbitReviewStreamEvent) => void,
) {
	return listen<CodeRabbitReviewStreamEvent>(
		CODERABBIT_REVIEW_EVENT_NAME,
		(event) => handler(event.payload),
	);
}

export function listLocalBranches(input: ListLocalBranchesInput) {
	return invoke<ListLocalBranchesOutput>(WORKSPACE_METHODS.listLocalBranches, {
		input,
	});
}

export function listWorkspaces() {
	return invoke<ListWorkspacesOutput>(WORKSPACE_METHODS.listWorkspaces);
}

export function listRepositories() {
	return invoke<ListRepositoriesOutput>(WORKSPACE_METHODS.listRepositories);
}

export function listWorkspaceBundles() {
	return invoke<ListWorkspaceBundlesOutput>(WORKSPACE_METHODS.listWorkspaceBundles);
}

export function archiveWorkspaceBundle(bundleId: string) {
	return invoke<WorkspaceBundleStateOutput>(WORKSPACE_METHODS.archiveWorkspaceBundle, {
		input: { bundleId },
	});
}

export function restoreWorkspaceBundle(bundleId: string) {
	return invoke<WorkspaceBundleStateOutput>(WORKSPACE_METHODS.restoreWorkspaceBundle, {
		input: { bundleId },
	});
}

export function deleteWorkspaceBundle(bundleId: string) {
	return invoke<void>(WORKSPACE_METHODS.deleteWorkspaceBundle, {
		input: { bundleId },
	});
}

export function listGitTrackedFiles(input: ListGitTrackedFilesInput) {
	return invoke<ListGitTrackedFilesOutput>(WORKSPACE_METHODS.listGitTrackedFiles, {
		input,
	});
}

export function readWorkspaceFile(input: ReadWorkspaceFileInput) {
	return invoke<ReadWorkspaceFileOutput>(WORKSPACE_METHODS.readWorkspaceFile, {
		input,
	});
}

export function writeWorkspaceFile(input: WriteWorkspaceFileInput) {
	return invoke<WriteWorkspaceFileOutput>(WORKSPACE_METHODS.writeWorkspaceFile, {
		input,
	});
}

export function searchWorkspace(input: SearchWorkspaceInput) {
	return invoke<SearchWorkspaceOutput>(WORKSPACE_METHODS.searchWorkspace, {
		input,
	});
}

export function listMissionSpecs(input: ListMissionSpecsInput) {
	return invoke<ListMissionSpecsOutput>(WORKSPACE_METHODS.listMissionSpecs, {
		input,
	});
}

export function compileMissionSpecContext(input: CompileMissionSpecContextInput) {
	return invoke<CompileMissionSpecContextOutput>(
		WORKSPACE_METHODS.compileMissionSpecContext,
		{ input },
	);
}

export function missionSpecContextStatus(input: MissionSpecContextStatusInput) {
	return invoke<MissionSpecContextStatusOutput>(
		WORKSPACE_METHODS.missionSpecContextStatus,
		{ input },
	);
}

export function saveMissionValidation(input: SaveMissionValidationInput) {
	return invoke<SaveMissionValidationOutput>(
		WORKSPACE_METHODS.saveMissionValidation,
		{ input },
	);
}

export function listChildDirectories(input: ListChildDirectoriesInput) {
	return invoke<ListChildDirectoriesOutput>(WORKSPACE_METHODS.listChildDirectories, {
		input,
	});
}

export function workspaceGitStatus(input: WorkspaceGitStatusInput) {
	return invoke<WorkspaceGitStatusOutput>(WORKSPACE_METHODS.workspaceGitStatus, {
		input,
	});
}

export function workspaceGitConflictState(input: WorkspaceGitConflictStateInput) {
	return invoke<WorkspaceGitConflictStateOutput>(
		WORKSPACE_METHODS.workspaceGitConflictState,
		{ input },
	);
}

export function workspaceGitAcceptConflict(input: WorkspaceGitAcceptConflictInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGitAcceptConflict, { input });
}

export function workspaceGitMarkConflictResolved(
	input: WorkspaceGitMarkConflictResolvedInput,
) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGitMarkConflictResolved, { input });
}

export function workspaceGitAbortMerge(input: WorkspaceGitConflictStateInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGitAbortMerge, { input });
}

export function workspaceGitValidationConfig(input: WorkspaceGitConflictStateInput) {
	return invoke<WorkspaceGitValidationConfigOutput>(
		WORKSPACE_METHODS.workspaceGitValidationConfig,
		{ input },
	);
}

export function workspaceProjectAutomationConfig(
	input: WorkspaceGitConflictStateInput,
) {
	return invoke<WorkspaceProjectAutomationConfigOutput>(
		WORKSPACE_METHODS.workspaceProjectAutomationConfig,
		{ input },
	);
}

export function workspaceSaveProjectAutomation(
	input: WorkspaceSaveProjectAutomationInput,
) {
	return invoke<WorkspaceProjectAutomationConfigOutput>(
		WORKSPACE_METHODS.workspaceSaveProjectAutomation,
		{ input },
	);
}

export function workspaceRunProjectTasks(input: WorkspaceRunProjectTasksInput) {
	return invoke<WorkspaceRunProjectTasksOutput>(
		WORKSPACE_METHODS.workspaceRunProjectTasks,
		{ input },
	);
}

export function workspaceGitCompleteMerge(input: WorkspaceGitCompleteMergeInput) {
	return invoke<WorkspaceGitCompleteMergeOutput>(
		WORKSPACE_METHODS.workspaceGitCompleteMerge,
		{ input },
	);
}

export function workspaceApplyDelegationWorktree(
	input: WorkspaceApplyDelegationWorktreeInput,
) {
	return invoke<WorkspaceApplyDelegationWorktreeOutput>(
		WORKSPACE_METHODS.workspaceApplyDelegationWorktree,
		{ input },
	);
}

export function workspacePrepareDelegationWorktree(
	input: WorkspacePrepareDelegationWorktreeInput,
) {
	return invoke<WorkspacePrepareDelegationWorktreeOutput>(
		WORKSPACE_METHODS.workspacePrepareDelegationWorktree,
		{ input },
	);
}

export function workspaceRemoveDelegationWorktree(
	input: WorkspaceRemoveDelegationWorktreeInput,
) {
	return invoke<void>(WORKSPACE_METHODS.workspaceRemoveDelegationWorktree, { input });
}

export function workspaceGitStageFile(input: WorkspaceGitPathInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGitStageFile, { input });
}

export function workspaceGitStageAll(input: WorkspaceGitPathInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGitStageAll, { input });
}

export function workspaceGitUnstageFile(input: WorkspaceGitPathInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGitUnstageFile, { input });
}

export function workspaceGitDiscardFile(input: WorkspaceGitPathInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGitDiscardFile, { input });
}

export function workspaceGitCommitPush(input: WorkspaceGitCommitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGitCommitPush, { input });
}

export function workspaceGitPush(input: WorkspaceGitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGitPush, { input });
}

export function workspaceGitSyncBase(input: WorkspaceGitSyncBaseInput) {
	return invoke<WorkspaceGitSyncBaseOutput>(WORKSPACE_METHODS.workspaceGitSyncBase, {
		input,
	});
}

export function workspaceContinueFromBaseBranch(input: WorkspaceContinueFromBaseBranchInput) {
	return invoke<WorkspaceContinueFromBaseBranchOutput>(
		WORKSPACE_METHODS.workspaceContinueFromBaseBranch,
		{ input },
	);
}

export function workspaceRunSetup(input: WorkspaceRunSetupInput) {
	return invoke<WorkspaceRunSetupOutput>(WORKSPACE_METHODS.workspaceRunSetup, {
		input,
	});
}

export function workspaceChangeRequestViewWeb(input: WorkspaceGitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceChangeRequestViewWeb, { input });
}

export function workspaceChangeRequestCreate(input: WorkspaceGitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceChangeRequestCreate, { input });
}

export function workspaceChangeRequestMerge(input: WorkspaceGitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceChangeRequestMerge, { input });
}

export function workspaceGhPrViewWeb(input: WorkspaceGitPushInput) {
	return workspaceChangeRequestViewWeb(input);
}

export function workspaceGhPrCreateFill(input: WorkspaceGitPushInput) {
	return workspaceChangeRequestCreate(input);
}

export function workspaceGhPrMerge(input: WorkspaceGitPushInput) {
	return workspaceChangeRequestMerge(input);
}

export function workspacePrStatus(input: WorkspacePrStatusInput) {
	return invoke<WorkspacePrStatusOutput>(WORKSPACE_METHODS.workspacePrStatus, {
		input,
	});
}

export function workspacePrReviewComments(input: WorkspacePrReviewCommentsInput) {
	return invoke<WorkspacePrReviewCommentsOutput>(
		WORKSPACE_METHODS.workspacePrReviewComments,
		{ input },
	);
}

export function workspacePipelineStatus(input: WorkspacePipelineStatusInput) {
	return invoke<WorkspacePipelineStatusOutput>(
		WORKSPACE_METHODS.workspacePipelineStatus,
		{ input },
	);
}

export function workspacePipelineJobLog(input: WorkspacePipelineJobInput) {
	return invoke<WorkspacePipelineJobLogOutput>(
		WORKSPACE_METHODS.workspacePipelineJobLog,
		{ input },
	);
}

export function workspacePipelineJobRetry(input: WorkspacePipelineJobInput) {
	return invoke<void>(WORKSPACE_METHODS.workspacePipelineJobRetry, { input });
}

export function workspaceReviewState(input: WorkspaceReviewStateInput) {
	return invoke<WorkspaceReviewStateOutput>(
		WORKSPACE_METHODS.workspaceReviewState,
		{ input },
	);
}

export function workspaceDeliveryFailureSnapshot(
	input: WorkspaceDeliveryFailureInput,
) {
	return invoke<WorkspaceDeliveryFailureOutput>(
		WORKSPACE_METHODS.workspaceDeliveryFailureSnapshot,
		{ input },
	);
}

export function workspaceDeliveryRecoveryExecute(
	input: WorkspaceDeliveryRecoveryInput,
) {
	return invoke<WorkspaceDeliveryRecoveryOutput>(
		WORKSPACE_METHODS.workspaceDeliveryRecoveryExecute,
		{ input },
	);
}

export function workspaceGitBranchDiff(input: WorkspaceGitBranchDiffInput) {
	return invoke<WorkspaceGitBranchDiffOutput>(WORKSPACE_METHODS.workspaceGitBranchDiff, {
		input,
	});
}

export function workspaceGitFilePreview(input: WorkspaceGitFilePreviewInput) {
	return invoke<string>(WORKSPACE_METHODS.workspaceGitFilePreview, { input });
}

export function workspaceGitFilePreviewContent(input: WorkspaceGitFilePreviewInput) {
	return invoke<WorkspaceGitFilePreviewContentOutput>(
		WORKSPACE_METHODS.workspaceGitFilePreviewContent,
		{ input },
	);
}
