import type { WorkspaceSummary, WorkspaceTone } from "./types";

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
