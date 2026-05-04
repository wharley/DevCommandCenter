import { invoke } from "@tauri-apps/api/core";
import { WORKSPACE_METHODS } from "@dcc/contracts";
import type {
	CreateWorkspaceForRepoInput,
	CreateWorkspaceForRepoOutput,
	CreateWorkspaceFromUrlInput,
	CreateWorkspaceFromUrlOutput,
	GithubCliStatusInput,
	GithubCliStatusOutput,
	ListChildDirectoriesInput,
	ListChildDirectoriesOutput,
	ListGitTrackedFilesInput,
	ListGitTrackedFilesOutput,
	ListLocalBranchesInput,
	ListLocalBranchesOutput,
	ListWorkspacesOutput,
	WorkspaceGitBranchDiffInput,
	WorkspaceGitBranchDiffOutput,
	WorkspaceGitFilePreviewInput,
	WorkspaceGitFilePreviewContentOutput,
	WorkspaceGitCommitPushInput,
	WorkspaceGitPathInput,
	WorkspaceGitPushInput,
	WorkspaceGitStatusInput,
	WorkspaceGitStatusOutput,
	WorkspaceContinueFromBaseBranchInput,
	WorkspacePrStatusInput,
	WorkspacePrStatusOutput,
} from "@dcc/contracts";

export function createWorkspaceForRepo(input: CreateWorkspaceForRepoInput) {
	return invoke<CreateWorkspaceForRepoOutput>(WORKSPACE_METHODS.createWorkspaceForRepo, {
		input,
	});
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

export function listLocalBranches(input: ListLocalBranchesInput) {
	return invoke<ListLocalBranchesOutput>(WORKSPACE_METHODS.listLocalBranches, {
		input,
	});
}

export function listWorkspaces() {
	return invoke<ListWorkspacesOutput>(WORKSPACE_METHODS.listWorkspaces);
}

export function listGitTrackedFiles(input: ListGitTrackedFilesInput) {
	return invoke<ListGitTrackedFilesOutput>(WORKSPACE_METHODS.listGitTrackedFiles, {
		input,
	});
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

export function workspaceContinueFromBaseBranch(input: WorkspaceContinueFromBaseBranchInput) {
	return invoke<{ success: boolean; branch?: string; workspaceRoot?: string }>(
		WORKSPACE_METHODS.workspaceContinueFromBaseBranch,
		{ input },
	);
}

export function workspaceGhPrViewWeb(input: WorkspaceGitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGhPrViewWeb, { input });
}

export function workspaceGhPrCreateFill(input: WorkspaceGitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGhPrCreateFill, { input });
}

export function workspaceGhPrMerge(input: WorkspaceGitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGhPrMerge, { input });
}

export function workspacePrStatus(input: WorkspacePrStatusInput) {
	return invoke<WorkspacePrStatusOutput>(WORKSPACE_METHODS.workspacePrStatus, {
		input,
	});
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
