import { invoke } from "@tauri-apps/api/core";
import { WORKSPACE_METHODS } from "@dcc/contracts";
import type {
	CreateWorkspaceForRepoInput,
	CreateWorkspaceForRepoOutput,
	CreateWorkspaceFromUrlInput,
	CreateWorkspaceFromUrlOutput,
	ListChildDirectoriesInput,
	ListChildDirectoriesOutput,
	ListGitTrackedFilesInput,
	ListGitTrackedFilesOutput,
	ListLocalBranchesInput,
	ListLocalBranchesOutput,
	ListWorkspacesOutput,
	WorkspaceGitCommitPushInput,
	WorkspaceGitPathInput,
	WorkspaceGitPushInput,
	WorkspaceGitStatusInput,
	WorkspaceGitStatusOutput,
} from "@dcc/contracts";

export function createWorkspaceForRepo(input: CreateWorkspaceForRepoInput) {
	return invoke<CreateWorkspaceForRepoOutput>(WORKSPACE_METHODS.createWorkspaceForRepo, {
		input,
	});
}

export function createWorkspaceFromUrl(input: CreateWorkspaceFromUrlInput) {
	return invoke<CreateWorkspaceFromUrlOutput>(WORKSPACE_METHODS.createWorkspaceFromUrl, {
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

export function workspaceGhPrViewWeb(input: WorkspaceGitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGhPrViewWeb, { input });
}

export function workspaceGhPrCreateFill(input: WorkspaceGitPushInput) {
	return invoke<void>(WORKSPACE_METHODS.workspaceGhPrCreateFill, { input });
}
