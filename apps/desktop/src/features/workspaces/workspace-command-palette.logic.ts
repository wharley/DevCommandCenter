import type {
	SessionSearchResult,
	WorkspaceSessionSummary,
} from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";

export type PaletteAction = {
	label: string;
	keywords: string;
};

export const PALETTE_SESSION_SEARCH_MIN_LENGTH = 2;
export const MAX_RECENT_PALETTE_SESSIONS = 40;

export function resolvePaletteSessionSearch(query: string) {
	const match = query.trimStart().match(/^@\s*(.*)$/);
	const term = match?.[1]?.trim() ?? "";
	return {
		query: term,
		isExplicit: match !== null,
		enabled:
			match !== null && term.length >= PALETTE_SESSION_SEARCH_MIN_LENGTH,
	};
}

function queryTokens(query: string) {
	return query.trim().toLowerCase().split(/\s+/).filter(Boolean);
}

/** Keep the selected project discoverable while favouring precise project matches. */
export function rankPaletteWorkspaces(
	workspaces: readonly WorkspaceSummary[],
	query: string,
	selectedWorkspaceId: string | null,
): WorkspaceSummary[] {
	const tokens = queryTokens(query);
	return workspaces
		.filter(
			(workspace) =>
				workspace.status !== "archived" && workspace.status !== "completed",
		)
		.map((workspace) => {
			const haystack = `${workspace.name} ${workspace.branch} ${workspace.id}`.toLowerCase();
			if (tokens.some((token) => !haystack.includes(token))) return null;
			const name = workspace.name.toLowerCase();
			const branch = workspace.branch.toLowerCase();
			const score =
				(tokens.length === 1 && name === tokens[0] ? 100 : 0) +
				tokens.reduce(
					(total, token) =>
						total +
						(name.startsWith(token) ? 20 : 0) +
						(name.includes(token) ? 8 : 0) +
						(branch.includes(token) ? 4 : 0),
					0,
				) +
				(workspace.id === selectedWorkspaceId ? 1 : 0);
			return { workspace, score };
		})
		.filter((entry): entry is { workspace: WorkspaceSummary; score: number } => entry !== null)
		.sort(
			(a, b) =>
				b.score - a.score ||
				a.workspace.name.localeCompare(b.workspace.name) ||
				a.workspace.branch.localeCompare(b.workspace.branch),
		)
		.map((entry) => entry.workspace);
}

export function matchesPaletteAction(
	action: PaletteAction,
	query: string,
): boolean {
	const tokens = queryTokens(query);
	return (
		tokens.length === 0 ||
		tokens.every((token) =>
			`${action.label} ${action.keywords}`.toLowerCase().includes(token),
		)
	);
}

/**
 * The command palette can show the sessions already loaded for the active
 * workspace without issuing a history/FTS query. It intentionally has no
 * prompt/snippet payload; that only exists after an explicit @ search.
 */
export function recentPaletteSessions(
	sessions: readonly WorkspaceSessionSummary[],
	workspace: Pick<WorkspaceSummary, "name" | "branch" | "rootPath" | "worktreePath"> | null,
): SessionSearchResult[] {
	return sessions
		.map((summary) => ({
			sessionId: summary.session.id,
			workspaceId: summary.session.workspaceId,
			projectId: summary.session.projectId,
			threadTitle: summary.thread.title,
			workspaceName: workspace?.name ?? null,
			workspaceBranch: workspace?.branch ?? null,
			workspaceRootPath: workspace?.worktreePath ?? workspace?.rootPath ?? null,
			providerId: summary.session.providerId,
			model: summary.session.model,
			archivedAt: summary.thread.archived_at,
			createdAt: summary.session.createdAt,
			updatedAt: summary.session.updatedAt,
			snippet: "",
		}))
		.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
		.slice(0, MAX_RECENT_PALETTE_SESSIONS);
}

export function filterPaletteSessionsByMetadata(
	results: readonly SessionSearchResult[],
	query: string,
): SessionSearchResult[] {
	const tokens = queryTokens(query);
	return results.filter((result) => {
		const metadata = `${result.threadTitle} ${result.workspaceName ?? ""} ${result.workspaceBranch ?? ""}`.toLowerCase();
		return tokens.every((token) => metadata.includes(token));
	});
}

export function groupPaletteSessions(
	results: readonly SessionSearchResult[],
	selectedWorkspaceId: string | null,
) {
	if (!selectedWorkspaceId) {
		return { currentWorkspace: [], otherWorkspaces: [...results] };
	}

	return {
		currentWorkspace: results.filter(
			(result) => result.workspaceId === selectedWorkspaceId,
		),
		otherWorkspaces: results.filter(
			(result) => result.workspaceId !== selectedWorkspaceId,
		),
	};
}
