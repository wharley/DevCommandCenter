import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { WorkspaceStatus, WorkspaceSummary } from "./types";
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
	WorkspaceSetupHint,
	WorkspaceSetupReport,
} from "@dcc/contracts";

export type WorkspaceCreationResult = {
	workspace: WorkspaceSummary;
	setupHints: WorkspaceSetupHint[];
	setupReport: WorkspaceSetupReport;
};

function applyStatusOverride(
	workspace: WorkspaceSummary,
	statusOverrides: Partial<Record<string, WorkspaceStatus>>,
): WorkspaceSummary {
	const nextStatus = statusOverrides[workspace.id];
	return nextStatus && nextStatus !== workspace.status
		? { ...workspace, status: nextStatus }
		: workspace;
}

function removeStatusOverrides(
	statusOverrides: Partial<Record<string, WorkspaceStatus>>,
	workspaceIds: readonly string[],
): Partial<Record<string, WorkspaceStatus>> {
	if (workspaceIds.length === 0) {
		return statusOverrides;
	}
	let changed = false;
	const nextOverrides = { ...statusOverrides };
	for (const workspaceId of workspaceIds) {
		if (workspaceId in nextOverrides) {
			delete nextOverrides[workspaceId];
			changed = true;
		}
	}
	return changed ? nextOverrides : statusOverrides;
}

function mergeWorkspaceSummaries(
	workspaces: WorkspaceSummary[],
	optimisticCreated: WorkspaceSummary[],
	statusOverrides: Partial<Record<string, WorkspaceStatus>>,
	hiddenWorkspaceIds: readonly string[],
): WorkspaceSummary[] {
	const hiddenIds = new Set(hiddenWorkspaceIds);
	const backendIds = new Set(workspaces.map((workspace) => workspace.id));
	const mergedCreated = optimisticCreated
		.filter((workspace) => !hiddenIds.has(workspace.id) && !backendIds.has(workspace.id))
		.map((workspace) => applyStatusOverride(workspace, statusOverrides));
	const mergedBackend = workspaces
		.filter((workspace) => !hiddenIds.has(workspace.id))
		.map((workspace) => applyStatusOverride(workspace, statusOverrides));
	return [...mergedCreated, ...mergedBackend];
}

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
		setupReport: workspace.setupReport,
		createdAt: workspace.createdAt,
		updatedAt: workspace.updatedAt,
	};
}

export function useWorkspacesPanel(workspaces: WorkspaceSummary[] = []) {
	const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(
		workspaces[0]?.id ?? null,
	);
	const [optimisticCreated, setOptimisticCreated] = useState<WorkspaceSummary[]>([]);
	const [hiddenWorkspaceIds, setHiddenWorkspaceIds] = useState<string[]>([]);
	const [statusOverrides, setStatusOverrides] = useState<
		Partial<Record<string, WorkspaceStatus>>
	>({});
	const [isCreatingWorkspace, setIsCreatingWorkspace] = useState(false);
	const workspaceList = useMemo(
		() =>
			mergeWorkspaceSummaries(
				workspaces,
				optimisticCreated,
				statusOverrides,
				hiddenWorkspaceIds,
			),
		[hiddenWorkspaceIds, optimisticCreated, statusOverrides, workspaces],
	);
	const workspaceListRef = useRef(workspaceList);
	const selectedWorkspaceIdRef = useRef(selectedWorkspaceId);

	useEffect(() => {
		workspaceListRef.current = workspaceList;
	}, [workspaceList]);

	useEffect(() => {
		selectedWorkspaceIdRef.current = selectedWorkspaceId;
	}, [selectedWorkspaceId]);

	useEffect(() => {
		const backendIds = new Set(workspaces.map((workspace) => workspace.id));
		setOptimisticCreated((current) => {
			const next = current.filter((workspace) => !backendIds.has(workspace.id));
			return next.length === current.length ? current : next;
		});
	}, [workspaces]);

	useEffect(() => {
		setSelectedWorkspaceId((current) => {
			const nextSelectedWorkspaceId =
				current && workspaceList.some((workspace) => workspace.id === current)
					? current
					: workspaceList[0]?.id ?? null;
			return current === nextSelectedWorkspaceId ? current : nextSelectedWorkspaceId;
		});
	}, [workspaceList]);

	const filteredWorkspaces = workspaceList;

	const selectedWorkspace =
		filteredWorkspaces.find((workspace) => workspace.id === selectedWorkspaceId) ?? null;

	const createWorkspace = useCallback(async (input: CreateWorkspaceForRepoInput) => {
		setIsCreatingWorkspace(true);
		try {
			const result = await createWorkspaceForRepo(input);
			const summary = workspaceToSummary(result.workspace);
			setOptimisticCreated((current) => [
				summary,
				...current.filter((workspace) => workspace.id !== summary.id),
			]);
			setHiddenWorkspaceIds((current) =>
				current.includes(summary.id)
					? current.filter((workspaceId) => workspaceId !== summary.id)
					: current,
			);
			setStatusOverrides((current) => removeStatusOverrides(current, [summary.id]));
			setSelectedWorkspaceId(summary.id);
			return {
				workspace: summary,
				setupHints: result.setupHints,
				setupReport: result.setupReport,
			} satisfies WorkspaceCreationResult;
		} finally {
			setIsCreatingWorkspace(false);
		}
	}, []);

	const cloneWorkspaceFromUrl = useCallback(async (input: CreateWorkspaceFromUrlInput) => {
		setIsCreatingWorkspace(true);
		try {
			const result = await createWorkspaceFromUrl(input);
			const summary = workspaceToSummary(result.workspace);
			setOptimisticCreated((current) => [
				summary,
				...current.filter((workspace) => workspace.id !== summary.id),
			]);
			setHiddenWorkspaceIds((current) =>
				current.includes(summary.id)
					? current.filter((workspaceId) => workspaceId !== summary.id)
					: current,
			);
			setStatusOverrides((current) => removeStatusOverrides(current, [summary.id]));
			setSelectedWorkspaceId(summary.id);
			return {
				workspace: summary,
				setupHints: result.setupHints,
				setupReport: result.setupReport,
			} satisfies WorkspaceCreationResult;
		} finally {
			setIsCreatingWorkspace(false);
		}
	}, []);

	const archiveWorkspace = useCallback(async (workspaceId: string) => {
		await apiArchiveWorkspace(workspaceId);
		setStatusOverrides((current) =>
			current[workspaceId] === "archived"
				? current
				: { ...current, [workspaceId]: "archived" }
		);
	}, []);

	const restoreWorkspace = useCallback(async (workspaceId: string) => {
		await apiRestoreWorkspace(workspaceId);
		setStatusOverrides((current) =>
			current[workspaceId] === "ready" ? current : { ...current, [workspaceId]: "ready" }
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
			setOptimisticCreated((current) =>
				current.filter((workspace) => workspace.id !== workspaceId),
			);
			setHiddenWorkspaceIds((current) =>
				current.includes(workspaceId) ? current : [...current, workspaceId],
			);
			setStatusOverrides((current) => removeStatusOverrides(current, [workspaceId]));
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
			const idsToRemove = new Set(uniqueWorkspaceIds);
			setOptimisticCreated((current) =>
				current.filter((workspace) => !idsToRemove.has(workspace.id)),
			);
			setHiddenWorkspaceIds((current) => [
				...current,
				...uniqueWorkspaceIds.filter((workspaceId) => !current.includes(workspaceId)),
			]);
			setStatusOverrides((current) => removeStatusOverrides(current, uniqueWorkspaceIds));
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
