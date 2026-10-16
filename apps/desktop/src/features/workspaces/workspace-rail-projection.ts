import { isSnoozed } from "./workspace-snooze";
import type { Repository } from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";
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
	/** The tasks the single list shows: neither on hold nor completed. */
	activeRows: DccWorkspaceRailRow[];
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

	return { activeGroups, activeRows: active, waitingRows, completedRows };
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

/** Where a task sits in the rail's single list: pinned first, then by day. */
export type DccTaskListSectionId = "pinned" | "today" | "yesterday" | "week" | "older";

export type DccTaskListSection = {
	id: DccTaskListSectionId;
	rows: DccWorkspaceRailRow[];
};

const TASK_LIST_SECTION_ORDER: readonly DccTaskListSectionId[] = [
	"pinned",
	"today",
	"yesterday",
	"week",
	"older",
];

/**
 * When the user last worked on the task: their newest turn, or its creation.
 * Agent progress and backend metadata updates do not count, so a task only
 * moves up because of something the user did.
 */
export function taskInteractionMs(
	workspace: WorkspaceSummary,
	lastInteractionAt: string | null | undefined,
): number {
	let latest = Number.NEGATIVE_INFINITY;
	for (const value of [lastInteractionAt, workspace.createdAt]) {
		const ms = value ? Date.parse(value) : Number.NaN;
		if (!Number.isNaN(ms) && ms > latest) latest = ms;
	}
	return latest;
}

function startOfLocalDay(ms: number): number {
	const date = new Date(ms);
	date.setHours(0, 0, 0, 0);
	return date.getTime();
}

export function taskListSectionFor(
	workspace: WorkspaceSummary,
	interactionMs: number,
	now: number,
): DccTaskListSectionId {
	if (workspace.pinnedAt) return "pinned";
	const today = startOfLocalDay(now);
	if (interactionMs >= today) return "today";
	const yesterday = startOfLocalDay(today - 1);
	if (interactionMs >= yesterday) return "yesterday";
	if (interactionMs >= startOfLocalDay(today - 6 * 24 * 60 * 60 * 1000)) return "week";
	return "older";
}

/**
 * The rail's spine: every active task in one list, whatever its project,
 * newest interaction first and split by day. Pinned tasks stay on top in pin
 * order. `frozen` holds the list as it was when the pointer entered the rail,
 * so nothing moves under the cursor: known tasks keep their place, tasks that
 * appear meanwhile go on top of their section, and a pin (the user's own
 * click) still takes effect.
 */
export function projectWorkspaceTaskList(
	rows: readonly DccWorkspaceRailRow[],
	lastInteractionAt: Readonly<Record<string, string | null | undefined>> = {},
	now: number = Date.now(),
	frozen: readonly { id: string; section: DccTaskListSectionId }[] | null = null,
): DccTaskListSection[] {
	const ranked = rows
		.map((row) => ({ row, ms: taskInteractionMs(row, lastInteractionAt[row.id]) }))
		.sort((a, b) => {
			if (a.row.pinnedAt && b.row.pinnedAt) {
				return a.row.pinnedAt.localeCompare(b.row.pinnedAt) || a.row.id.localeCompare(b.row.id);
			}
			return b.ms - a.ms || a.row.id.localeCompare(b.row.id);
		});
	const frozenIndex = new Map(frozen?.map((entry, index) => [entry.id, { ...entry, index }]));
	const bySection = new Map<DccTaskListSectionId, { row: DccWorkspaceRailRow; rank: number }[]>();
	ranked.forEach(({ row, ms }, position) => {
		const live = taskListSectionFor(row, ms, now);
		const known = frozenIndex.get(row.id);
		// A pin change is the user's click and applies at once.
		const keepsPlace = known && (known.section === "pinned") === (live === "pinned");
		const section = keepsPlace ? known.section : live;
		const rank = keepsPlace ? known.index : position - ranked.length;
		const list = bySection.get(section);
		if (list) list.push({ row, rank });
		else bySection.set(section, [{ row, rank }]);
	});
	return TASK_LIST_SECTION_ORDER.flatMap((id) => {
		const entries = bySection.get(id);
		if (!entries?.length) return [];
		return [{ id, rows: entries.sort((a, b) => a.rank - b.rank).map((entry) => entry.row) }];
	});
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

function hashId(key: string): string {
	let h = 0;
	for (let i = 0; i < key.length; i++) {
		h = (Math.imul(31, h) + key.charCodeAt(i)) | 0;
	}
	return Math.abs(h).toString(36);
}
