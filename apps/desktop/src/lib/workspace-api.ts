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
