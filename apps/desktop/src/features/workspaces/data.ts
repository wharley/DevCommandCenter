import type { WorkspaceSummary, WorkspaceTone } from "./types";

export const demoWorkspaces: WorkspaceSummary[] = [
	{
		id: "ws_01",
		name: "Alpha",
		status: "ready",
		branch: "main",
		rootPath: "/projects/alpha-repo",
	},
	{
		id: "ws_02",
		name: "Core Refactor",
		status: "setup_pending",
		branch: "feat/core",
		rootPath: "/projects/alpha-repo",
	},
	{
		id: "ws_03",
		name: "Provider Swap",
		status: "initializing",
		branch: "feat/providers",
		rootPath: "/projects/beta-app",
	},
	{
		id: "ws_04",
		name: "Session Replay",
		status: "ready",
		branch: "feat/sessions",
		rootPath: "/projects/beta-app",
	},
	{
		id: "ws_05",
		name: "Archived Spike",
		status: "archived",
		branch: "chore/archive",
	},
];

export function getWorkspaceTone(status: WorkspaceSummary["status"]): WorkspaceTone {
	switch (status) {
		case "ready":
			return "success";
	case "setup_pending":
		return "warn";
	case "archived":
		return "secondary";
	default:
		return "secondary";
	}
}
