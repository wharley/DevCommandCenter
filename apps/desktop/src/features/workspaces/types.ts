export type WorkspaceStatus = "ready" | "setup_pending" | "initializing" | "archived";

export interface WorkspaceSummary {
	id: string;
	name: string;
	branch: string;
	status: WorkspaceStatus;
}

export type WorkspaceTone = "success" | "warn" | "secondary";
