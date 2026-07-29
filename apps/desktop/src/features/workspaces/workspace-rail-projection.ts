import type { Repository } from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";

export type DccWorkspaceRailRow = WorkspaceSummary;

export type DccWorkspaceRailGroup = {
	id: string;
	label: string;
	sourceKey: string;
	rows: DccWorkspaceRailRow[];
};

export type DccWorkspaceRepository = {
	sourceKey: string;
	label: string;
	projectId: string;
	workspaceRoot: string;
	branch: string;
	updatedAt: string;
};

export function projectGroupingKey(workspace: WorkspaceSummary): string {
	return (
		workspace.rootPath?.trim() ||
		workspace.worktreePath?.trim() ||
		workspace.projectId?.trim() ||
		`workspace:${workspace.id}`
	);
}

function projectGroupingLabel(workspace: WorkspaceSummary): string {
	const path = workspace.rootPath?.trim() || workspace.worktreePath?.trim();
	if (path) {
		const segments = path.split(/[/\\]/).filter(Boolean);
		const leaf = segments.at(-1);
		return leaf ?? path;
	}
	if (workspace.projectId?.trim()) {
		return workspace.projectId.trim();
	}
	return workspace.name.trim() || "Workspace";
}

export function projectWorkspaceRepositories(
	workspaces: WorkspaceSummary[],
): DccWorkspaceRepository[] {
	const byKey = new Map<string, WorkspaceSummary[]>();

	for (const workspace of workspaces) {
		const workspaceRoot = workspace.rootPath?.trim();
		const projectId = workspace.projectId?.trim();
		if (!workspaceRoot || !projectId) {
			continue;
		}

		const key = projectGroupingKey(workspace);
		const list = byKey.get(key);
		if (list) {
			list.push(workspace);
		} else {
			byKey.set(key, [workspace]);
		}
	}

	return [...byKey.entries()]
		.map(([sourceKey, entries]) => {
			const sorted = [...entries].sort((a, b) => {
				const ta = a.updatedAt ?? a.createdAt ?? a.name;
				const tb = b.updatedAt ?? b.createdAt ?? b.name;
				return tb.localeCompare(ta);
			});
			const representative = sorted[0]!;
			return {
				sourceKey,
				label: projectGroupingLabel(representative),
				projectId: representative.projectId!.trim(),
				workspaceRoot: representative.rootPath!.trim(),
				branch: representative.branch,
				updatedAt:
					representative.updatedAt ?? representative.createdAt ?? representative.name,
			};
		})
		.sort((a, b) => {
			const updatedAtOrder = b.updatedAt.localeCompare(a.updatedAt);
			if (updatedAtOrder !== 0) {
				return updatedAtOrder;
			}
			return a.label.localeCompare(b.label);
		});
}

/**
 * Sidebar spine: group active workspaces by project path (t3-style),
 * paused and completed rows lifted into dedicated sections handled by the shell component.
 */
export function projectWorkspaceRailGroups(
	workspaces: WorkspaceSummary[],
	repositories: Repository[] = [],
): {
	activeGroups: DccWorkspaceRailGroup[];
	waitingRows: DccWorkspaceRailRow[];
	completedRows: DccWorkspaceRailRow[];
} {
	const waitingRows = workspaces.filter((workspace) => workspace.status === "archived");
	const completedRows = workspaces.filter((workspace) => workspace.status === "completed");
	const active = workspaces.filter(
		(workspace) => workspace.status !== "archived" && workspace.status !== "completed",
	);

	const byKey = new Map<string, WorkspaceSummary[]>();
	for (const repository of repositories) {
		const key = repository.rootPath.trim();
		if (!key) {
			continue;
		}
		byKey.set(key, []);
	}

	for (const workspace of active) {
		const key = projectGroupingKey(workspace);
		const list = byKey.get(key);
		if (list) {
			list.push(workspace);
		} else {
			byKey.set(key, [workspace]);
		}
	}

	const activeGroups: DccWorkspaceRailGroup[] = [...byKey.entries()]
		.map(([key, rows]) => {
			const sorted = [...rows].sort((a, b) => {
				const ta = a.updatedAt ?? a.createdAt ?? a.name;
				const tb = b.updatedAt ?? b.createdAt ?? b.name;
				return tb.localeCompare(ta);
			});
			const repository = repositories.find((candidate) => candidate.rootPath.trim() === key) ?? null;
			const label = repository?.name?.trim() || (sorted[0] ? projectGroupingLabel(sorted[0]) : key);
			return {
				id: `dcc.proj.${hashId(key)}`,
				label,
				sourceKey: key,
				rows: sorted,
			};
		})
		.sort((a, b) => a.label.localeCompare(b.label));

	return { activeGroups, waitingRows, completedRows };
}

function hashId(key: string): string {
	let h = 0;
	for (let i = 0; i < key.length; i++) {
		h = (Math.imul(31, h) + key.charCodeAt(i)) | 0;
	}
	return Math.abs(h).toString(36);
}
