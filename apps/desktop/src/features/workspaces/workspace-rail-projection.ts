import { isSnoozed } from "./workspace-snooze";
import type { Repository } from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";
import type { WorkspaceAgentActivity } from "./use-workspace-agent-states";
import { repositoryDisplayName } from "./repository-display-name";

export type DccWorkspaceRailRow = WorkspaceSummary;

export type DccWorkspaceRailGroup = {
	id: string;
	label: string;
	sourceKey: string;
	pinnedAt: string | null;
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
	lastInteractionAt: Readonly<Record<string, string | null | undefined>> = {},
	now: number = Date.now(),
): {
	activeGroups: DccWorkspaceRailGroup[];
	waitingRows: DccWorkspaceRailRow[];
	completedRows: DccWorkspaceRailRow[];
} {
	// Snoozed tasks wait with the paused ones until their time comes.
	const waitingRows = workspaces.filter(
		(workspace) =>
			workspace.status === "archived" ||
			(workspace.status !== "completed" && isSnoozed(workspace, now)),
	);
	const completedRows = workspaces.filter((workspace) => workspace.status === "completed");
	const active = workspaces.filter(
		(workspace) =>
			workspace.status !== "archived" &&
			workspace.status !== "completed" &&
			!isSnoozed(workspace, now),
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
				const pinnedOrder = Number(Boolean(b.pinnedAt)) - Number(Boolean(a.pinnedAt));
				if (pinnedOrder !== 0) return pinnedOrder;
				return (
					workspaceRecencyMs(b, lastInteractionAt[b.id]) -
						workspaceRecencyMs(a, lastInteractionAt[a.id]) ||
					a.id.localeCompare(b.id)
				);
			});
			const repository = repositories.find((candidate) => candidate.rootPath.trim() === key) ?? null;
			const label = repository
				? repositoryDisplayName(repository)
				: sorted[0]
					? projectGroupingLabel(sorted[0])
					: key;
			return {
				id: `dcc.proj.${hashId(key)}`,
				label,
				sourceKey: key,
				pinnedAt: repository?.pinnedAt ?? null,
				rows: sorted,
			};
		})
		.map((group) => ({
			group,
			recencyMs: projectRecencyMs(group.rows, lastInteractionAt),
		}))
		.sort((a, b) => {
			const pinnedOrder = Number(Boolean(b.group.pinnedAt)) - Number(Boolean(a.group.pinnedAt));
			if (pinnedOrder !== 0) return pinnedOrder;
			// Pinned projects stay where you put them: oldest pin first.
			if (a.group.pinnedAt && b.group.pinnedAt) {
				return (
					a.group.pinnedAt.localeCompare(b.group.pinnedAt) ||
					a.group.label.localeCompare(b.group.label)
				);
			}
			return b.recencyMs - a.recencyMs || a.group.label.localeCompare(b.group.label);
		})
		.map(({ group }) => group);

	return { activeGroups, waitingRows, completedRows };
}

/**
 * When the user last worked in the project: their newest turn in any of its
 * tasks, or a task's creation. Agent progress and backend metadata updates do
 * not count, so a project only moves up because of something the user did.
 */
function projectRecencyMs(
	rows: readonly DccWorkspaceRailRow[],
	lastInteractionAt: Readonly<Record<string, string | null | undefined>>,
): number {
	let latest = Number.NEGATIVE_INFINITY;
	for (const row of rows) {
		for (const value of [lastInteractionAt[row.id], row.createdAt]) {
			const ms = value ? Date.parse(value) : Number.NaN;
			if (!Number.isNaN(ms) && ms > latest) latest = ms;
		}
	}
	return latest;
}

/**
 * Holds the unpinned projects in the order they had when the pointer entered
 * the rail, so nothing moves under the cursor. Pinning stays live (it is the
 * user's own click); projects that appear meanwhile go on top, as the newest.
 */
export function freezeRailGroupOrder(
	groups: readonly DccWorkspaceRailGroup[],
	frozenIds: readonly string[],
): DccWorkspaceRailGroup[] {
	const frozenIndex = new Map(frozenIds.map((id, index) => [id, index]));
	const pinned = groups.filter((group) => group.pinnedAt);
	const unpinned = groups.filter((group) => !group.pinnedAt);
	const fresh = unpinned.filter((group) => !frozenIndex.has(group.id));
	const known = unpinned
		.filter((group) => frozenIndex.has(group.id))
		.sort((a, b) => frozenIndex.get(a.id)! - frozenIndex.get(b.id)!);
	return [...pinned, ...fresh, ...known];
}

/**
 * Recent projects without an active task step aside behind one "no tasks"
 * line, so the rail lists where work is. Pinned projects always stay: the pin
 * is the user asking for them. `keepIds` holds projects that were on screen
 * when the pointer entered the rail, so one that just ran out of tasks does
 * not vanish under the cursor.
 */
export function splitIdleRailGroups(
	groups: readonly DccWorkspaceRailGroup[],
	keepIds: ReadonlySet<string> = new Set(),
): { visibleGroups: DccWorkspaceRailGroup[]; idleGroups: DccWorkspaceRailGroup[] } {
	const visibleGroups: DccWorkspaceRailGroup[] = [];
	const idleGroups: DccWorkspaceRailGroup[] = [];
	for (const group of groups) {
		if (group.pinnedAt || group.rows.length > 0 || keepIds.has(group.id)) {
			visibleGroups.push(group);
		} else {
			idleGroups.push(group);
		}
	}
	return { visibleGroups, idleGroups };
}

/**
 * Most recent moment the task was touched: the user's last turn, or its own
 * creation/metadata update when it has no turns yet (a fresh task goes on top).
 */
function workspaceRecencyMs(
	workspace: WorkspaceSummary,
	lastInteractionAt: string | null | undefined,
): number {
	const candidates = [lastInteractionAt, workspace.updatedAt, workspace.createdAt]
		.map((value) => (value ? Date.parse(value) : Number.NaN))
		.filter((value) => !Number.isNaN(value));
	return candidates.length ? Math.max(...candidates) : Number.NEGATIVE_INFINITY;
}

/**
 * What a project header must still say when its rows are collapsed or scrolled
 * away. Amber is reserved for "needs you"; a running agent only earns green.
 */
export type DccWorkspaceRailGroupSignal = "attention" | "running" | null;

export function workspaceRailGroupSignal(
	rows: readonly DccWorkspaceRailRow[],
	activities: Readonly<Record<string, WorkspaceAgentActivity | null | undefined>>,
): DccWorkspaceRailGroupSignal {
	if (
		rows.some(
			(row) =>
				row.status === "setup_pending" ||
				activities[row.id]?.state === "waiting",
		)
	) {
		return "attention";
	}
	if (rows.some((row) => activities[row.id]?.state === "active")) {
		return "running";
	}
	return null;
}

function hashId(key: string): string {
	let h = 0;
	for (let i = 0; i < key.length; i++) {
		h = (Math.imul(31, h) + key.charCodeAt(i)) | 0;
	}
	return Math.abs(h).toString(36);
}
