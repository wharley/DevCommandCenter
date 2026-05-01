import { useCallback, useState } from "react";
import { demoWorkspaces } from "./data";
import type { WorkspaceSummary } from "./types";
import { createWorkspaceForRepo } from "../../lib/workspace-api";
import type { CreateWorkspaceForRepoInput, Workspace } from "@dcc/contracts";

function workspaceToSummary(workspace: Workspace): WorkspaceSummary {
	const status =
		workspace.state === "ready"
			? "ready"
			: workspace.state === "archived"
				? "archived"
				: workspace.state === "initializing"
					? "initializing"
					: "setup_pending";

	return {
		id: workspace.id,
		name: workspace.name ?? workspace.baseBranch,
		branch: workspace.baseBranch,
		status,
		projectId: workspace.projectId,
		rootPath: workspace.rootPath,
		worktreePath: workspace.worktreePath,
		createdAt: workspace.createdAt,
		updatedAt: workspace.updatedAt,
	};
}

export function useWorkspacesPanel(workspaces: WorkspaceSummary[] = demoWorkspaces) {
	const [selectedWorkspaceId, setSelectedWorkspaceId] = useState(
		workspaces[1]?.id ?? workspaces[0]?.id ?? "",
	);
	const [workspaceList, setWorkspaceList] = useState<WorkspaceSummary[]>(workspaces);
	const [isCreatingWorkspace, setIsCreatingWorkspace] = useState(false);

	const filteredWorkspaces = workspaceList;

	const selectedWorkspace =
		filteredWorkspaces.find((workspace) => workspace.id === selectedWorkspaceId) ??
		filteredWorkspaces[0] ??
		workspaceList[0];

	const createWorkspace = useCallback(async (input: CreateWorkspaceForRepoInput) => {
		setIsCreatingWorkspace(true);
		try {
			const result = await createWorkspaceForRepo(input);
			const summary = workspaceToSummary(result.workspace);
			setWorkspaceList((current) => [summary, ...current]);
			setSelectedWorkspaceId(summary.id);
			return summary;
		} finally {
			setIsCreatingWorkspace(false);
		}
	}, []);

	return {
		allWorkspaces: workspaceList,
		createWorkspace,
		isCreatingWorkspace,
		filteredWorkspaces,
		selectedWorkspace,
		selectedWorkspaceId,
		setSelectedWorkspaceId,
	};
}
