/** Workspace runtime session projection shown in workbench chrome (Rust core). */

export type DccRuntimeSessionSnapshot = {
	sessionId: string;
	projectId: string;
	workspaceId: string;
	providerId: string;
	model: string | null;
	state: string;
	turnCount: number;
	checkpointCount: number;
	lastTurnPrompt?: string | null;
	lastTurnState?: string | null;
};

export type RuntimeSessionSnapshot = DccRuntimeSessionSnapshot;
