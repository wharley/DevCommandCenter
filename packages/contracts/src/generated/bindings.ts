// Placeholder contract surface for phase 0a.
// This file will be replaced by generated bindings once tauri-specta is wired.

export type WorkspaceId = string;
export type ProjectId = string;
export type SessionId = string;
export type ProviderId = string;

export type WorkspaceState =
	| "initializing"
	| "setup_pending"
	| "ready"
	| "archived";

export interface Workspace {
	id: WorkspaceId;
	projectId: ProjectId;
	rootPath: string;
	baseBranch: string;
	worktreePath: string | null;
	state: WorkspaceState;
	createdAt: string;
	updatedAt: string;
}

export interface CreateWorkspaceForRepoInput {
	projectId: ProjectId;
	workspaceRoot: string;
	baseBranch: string;
	name?: string | null;
}

export interface CreateWorkspaceForRepoOutput {
	workspace: Workspace;
	worktreePath: string;
}
