import type { WorkspaceSummary } from "./types";

export type DccWorkspaceRailRow = WorkspaceSummary;

export type DccWorkspaceRailGroup = {
	id: string;
	label: string;
	rows: DccWorkspaceRailRow[];
};

function projectGroupingKey(workspace: WorkspaceSummary): string {
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

/**
 * Sidebar spine: group active workspaces by project path (t3-style),
 * archived rows lifted into a dedicated section handled by the shell component.
 */
export function projectWorkspaceRailGroups(
	workspaces: WorkspaceSummary[],
): {
	activeGroups: DccWorkspaceRailGroup[];
	archivedRows: DccWorkspaceRailRow[];
} {
	const archivedRows = workspaces.filter((w) => w.status === "archived");
	const active = workspaces.filter((w) => w.status !== "archived");

	const byKey = new Map<string, WorkspaceSummary[]>();
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
			const label = projectGroupingLabel(sorted[0]!);
			return {
				id: `dcc.proj.${hashId(key)}`,
				label,
				rows: sorted,
			};
		})
		.sort((a, b) => a.label.localeCompare(b.label));

	return { activeGroups, archivedRows };
}

function hashId(key: string): string {
	let h = 0;
	for (let i = 0; i < key.length; i++) {
		h = (Math.imul(31, h) + key.charCodeAt(i)) | 0;
	}
	return Math.abs(h).toString(36);
}
