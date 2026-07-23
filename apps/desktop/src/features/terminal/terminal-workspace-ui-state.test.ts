import { describe, expect, it } from "vitest";
import {
	getWorkspaceTerminalUiState,
	updateWorkspaceTerminalUiState,
	type WorkspaceTerminalUiStates,
} from "./terminal-workspace-ui-state";

describe("workspace terminal UI state", () => {
	it("keeps terminal visibility isolated when the active workspace changes", () => {
		const states = updateWorkspaceTerminalUiState({}, "workspace-a", {
			open: true,
			expanded: true,
			scopeKind: "worktree",
		});

		expect(getWorkspaceTerminalUiState(states, "workspace-b")).toEqual({
			open: false,
			expanded: false,
			scopeKind: "worktree",
		});
		expect(getWorkspaceTerminalUiState(states, "workspace-a")).toEqual({
			open: true,
			expanded: true,
			scopeKind: "worktree",
		});
	});

	it("restores each workspace's selected scope independently", () => {
		let states: WorkspaceTerminalUiStates = {};
		states = updateWorkspaceTerminalUiState(states, "workspace-a", {
			open: true,
			expanded: false,
			scopeKind: "project",
		});
		states = updateWorkspaceTerminalUiState(states, "workspace-b", {
			open: true,
			expanded: false,
			scopeKind: "worktree",
		});

		expect(getWorkspaceTerminalUiState(states, "workspace-a").scopeKind).toBe(
			"project",
		);
		expect(getWorkspaceTerminalUiState(states, "workspace-b").scopeKind).toBe(
			"worktree",
		);
	});
});
