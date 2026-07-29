import type { WorkspaceSetupReport } from "@dcc/contracts";

export type WorkspaceStatus =
	| "ready"
	| "setup_pending"
	| "initializing"
	| "archived"
	| "completed";

export interface WorkspaceSummary {
	id: string;
	name: string;
	branch: string;
	status: WorkspaceStatus;
	unreadSessionCount?: number;
	projectId?: string | null;
	rootPath?: string | null;
	worktreePath?: string | null;
	setupReport?: WorkspaceSetupReport | null;
	createdAt?: string;
	updatedAt?: string;
	bundleId?: string | null;
	additionalWorkspaceIds?: string[];
	memberWorkspaceIds?: string[];
	memberNames?: string[];
}

export type WorkspaceTone = "success" | "warn" | "secondary";
