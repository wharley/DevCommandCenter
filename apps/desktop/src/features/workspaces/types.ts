export type WorkspaceStatus = "ready" | "setup_pending" | "initializing" | "archived";

export interface WorkspaceSummary {
	id: string;
	name: string;
	branch: string;
	status: WorkspaceStatus;
	unreadSessionCount?: number;
	projectId?: string | null;
	rootPath?: string | null;
	worktreePath?: string | null;
	createdAt?: string;
	updatedAt?: string;
}

export type WorkspaceTone = "success" | "warn" | "secondary";
