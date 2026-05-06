import { useCallback, useEffect, useRef, useState } from "react";
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

export function removeWorkspacesFromList(
	workspaces: WorkspaceSummary[],
	workspaceIds: readonly string[],
	selectedWorkspaceId: string | null,
): {
	workspaceList: WorkspaceSummary[];
	selectedWorkspaceId: string | null;
} {
	if (workspaceIds.length === 0) {
		return {
			workspaceList: workspaces,
			selectedWorkspaceId,
		};
	}

	const idsToRemove = new Set(workspaceIds);
	const workspaceList = workspaces.filter((workspace) => !idsToRemove.has(workspace.id));
	const nextSelectedWorkspaceId =
		selectedWorkspaceId && !idsToRemove.has(selectedWorkspaceId)
			? selectedWorkspaceId
			: workspaceList[0]?.id ?? null;

	return {
		workspaceList,
		selectedWorkspaceId: nextSelectedWorkspaceId,
	};
}

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
	const workspaceListRef = useRef(workspaceList);
	const selectedWorkspaceIdRef = useRef(selectedWorkspaceId);

	useEffect(() => {
		workspaceListRef.current = workspaceList;
	}, [workspaceList]);

	useEffect(() => {
		selectedWorkspaceIdRef.current = selectedWorkspaceId;
	}, [selectedWorkspaceId]);

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
			const nextState = removeWorkspacesFromList(
				workspaceListRef.current,
				[workspaceId],
				selectedWorkspaceIdRef.current,
			);
			workspaceListRef.current = nextState.workspaceList;
			selectedWorkspaceIdRef.current = nextState.selectedWorkspaceId;
			setWorkspaceList(nextState.workspaceList);
			setSelectedWorkspaceId(nextState.selectedWorkspaceId);
		},
		[],
	);

	const deleteWorkspaces = useCallback(
		async (workspaceIds: string[]) => {
			const uniqueWorkspaceIds = [...new Set(workspaceIds)];
			if (uniqueWorkspaceIds.length === 0) {
				return;
			}

			await Promise.all(uniqueWorkspaceIds.map((workspaceId) => apiDeleteWorkspace(workspaceId)));
			const nextState = removeWorkspacesFromList(
				workspaceListRef.current,
				uniqueWorkspaceIds,
				selectedWorkspaceIdRef.current,
			);
			workspaceListRef.current = nextState.workspaceList;
			selectedWorkspaceIdRef.current = nextState.selectedWorkspaceId;
			setWorkspaceList(nextState.workspaceList);
			setSelectedWorkspaceId(nextState.selectedWorkspaceId);
		},
		[],
	);

	return {
		allWorkspaces: workspaceList,
		archiveWorkspace,
		cloneWorkspaceFromUrl,
		createWorkspace,
		deleteWorkspace,
		deleteWorkspaces,
		isCreatingWorkspace,
		filteredWorkspaces,
		restoreWorkspace,
		selectedWorkspace,
		selectedWorkspaceId,
		setSelectedWorkspaceId,
	};
}
