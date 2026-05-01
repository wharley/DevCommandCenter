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
	/** Matches core session projection — non-null while a turn is in flight. */
	activeTurnId: string | null;
	lastTurnPrompt?: string | null;
	lastTurnState?: string | null;
};

export type RuntimeSessionSnapshot = DccRuntimeSessionSnapshot;
