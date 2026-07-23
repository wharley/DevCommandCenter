import type { TerminalScopeKind } from "./terminal-scope";

export type WorkspaceTerminalUiState = {
	open: boolean;
	expanded: boolean;
	scopeKind: TerminalScopeKind;
};

export type WorkspaceTerminalUiStates = Record<string, WorkspaceTerminalUiState>;

export type WorkspaceTerminalUiStateUpdate =
	| WorkspaceTerminalUiState
	| ((current: WorkspaceTerminalUiState) => WorkspaceTerminalUiState);

export const DEFAULT_WORKSPACE_TERMINAL_UI_STATE: WorkspaceTerminalUiState = {
	open: false,
	expanded: false,
	scopeKind: "worktree",
};

export function getWorkspaceTerminalUiState(
	states: WorkspaceTerminalUiStates,
	workspaceKey: string,
): WorkspaceTerminalUiState {
	return states[workspaceKey] ?? DEFAULT_WORKSPACE_TERMINAL_UI_STATE;
}

export function updateWorkspaceTerminalUiState(
	states: WorkspaceTerminalUiStates,
	workspaceKey: string,
	update: WorkspaceTerminalUiStateUpdate,
): WorkspaceTerminalUiStates {
	const current = getWorkspaceTerminalUiState(states, workspaceKey);
	const next = typeof update === "function" ? update(current) : update;
	if (
		next.open === current.open &&
		next.expanded === current.expanded &&
		next.scopeKind === current.scopeKind
	) {
		return states;
	}
	return { ...states, [workspaceKey]: next };
}
