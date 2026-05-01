import { invoke } from "@tauri-apps/api/core";
import { WORKSPACE_METHODS } from "@dcc/contracts";
import type {
	CreateWorkspaceForRepoInput,
	CreateWorkspaceForRepoOutput,
} from "@dcc/contracts";

export function createWorkspaceForRepo(input: CreateWorkspaceForRepoInput) {
	return invoke<CreateWorkspaceForRepoOutput>(WORKSPACE_METHODS.createWorkspaceForRepo, {
		input,
	});
}
