import { useMemo, useState } from "react";
import { demoWorkspaces } from "./data";
import type { WorkspaceSummary } from "./types";

export function useWorkspacesPanel(workspaces: WorkspaceSummary[] = demoWorkspaces) {
	const [filter, setFilter] = useState("");
	const [selectedWorkspaceId, setSelectedWorkspaceId] = useState("ws_02");

	const filteredWorkspaces = useMemo(
		() =>
			workspaces.filter((workspace) =>
				`${workspace.name} ${workspace.branch}`.toLowerCase().includes(
					filter.toLowerCase(),
				),
			),
		[filter, workspaces],
	);

	const selectedWorkspace =
		filteredWorkspaces.find((workspace) => workspace.id === selectedWorkspaceId) ??
		filteredWorkspaces[0] ??
		workspaces[0];

	return {
		allWorkspaces: workspaces,
		filter,
		filteredWorkspaces,
		selectedWorkspace,
		selectedWorkspaceId,
		setFilter,
		setSelectedWorkspaceId,
	};
}
