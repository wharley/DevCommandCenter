import { useCallback, useEffect, useState } from "react";
import type { WorkspaceSummary } from "./types";
import {
	archiveWorkspace as apiArchiveWorkspace,
	createWorkspaceForRepo,
	createWorkspaceFromUrl,
	deleteWorkspace as apiDeleteWorkspace,
	restoreWorkspace as apiRestoreWorkspace,
} from "../../lib/workspace-api";
import type {
	CreateWorkspaceForRepoInput,
	CreateWorkspaceFromUrlInput,
	Workspace,
} from "@dcc/contracts";

export function workspaceToSummary(workspace: Workspace): WorkspaceSummary {
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

export function useWorkspacesPanel(workspaces: WorkspaceSummary[] = []) {
	const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(
		workspaces[0]?.id ?? null,
	);
	const [workspaceList, setWorkspaceList] = useState<WorkspaceSummary[]>(workspaces);
	const [isCreatingWorkspace, setIsCreatingWorkspace] = useState(false);

	useEffect(() => {
		if (workspaces.length === 0) {
			return;
		}

		setWorkspaceList((current) => (current.length === 0 ? workspaces : current));
		setSelectedWorkspaceId((current) => current ?? workspaces[0]?.id ?? null);
	}, [workspaces]);

	const filteredWorkspaces = workspaceList;

	const selectedWorkspace =
		filteredWorkspaces.find((workspace) => workspace.id === selectedWorkspaceId) ?? null;

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

	const cloneWorkspaceFromUrl = useCallback(async (input: CreateWorkspaceFromUrlInput) => {
		setIsCreatingWorkspace(true);
		try {
			const result = await createWorkspaceFromUrl(input);
			const summary = workspaceToSummary(result.workspace);
			setWorkspaceList((current) => [summary, ...current]);
			setSelectedWorkspaceId(summary.id);
			return summary;
		} finally {
			setIsCreatingWorkspace(false);
		}
	}, []);

	const archiveWorkspace = useCallback(async (workspaceId: string) => {
		await apiArchiveWorkspace(workspaceId);
		setWorkspaceList((current) =>
			current.map((w) => (w.id === workspaceId ? { ...w, status: "archived" as const } : w)),
		);
	}, []);

	const restoreWorkspace = useCallback(async (workspaceId: string) => {
		await apiRestoreWorkspace(workspaceId);
		setWorkspaceList((current) =>
			current.map((w) => (w.id === workspaceId ? { ...w, status: "ready" as const } : w)),
		);
	}, []);

	const deleteWorkspace = useCallback(
		async (workspaceId: string) => {
			await apiDeleteWorkspace(workspaceId);
			setWorkspaceList((current) => current.filter((w) => w.id !== workspaceId));
			if (selectedWorkspaceId === workspaceId) {
				setSelectedWorkspaceId(null);
			}
		},
		[selectedWorkspaceId],
	);

	return {
		allWorkspaces: workspaceList,
		archiveWorkspace,
		cloneWorkspaceFromUrl,
		createWorkspace,
		deleteWorkspace,
		isCreatingWorkspace,
		filteredWorkspaces,
		restoreWorkspace,
		selectedWorkspace,
		selectedWorkspaceId,
		setSelectedWorkspaceId,
	};
}
