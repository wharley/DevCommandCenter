import type { QueryClient } from "@tanstack/react-query";
import { SESSION_COMPOSER_MAP_KEY } from "@/features/providers/provider-selection.logic";

/** Per-task composer keys: `<prefix><workspaceId>` or `<prefix><workspaceId>.<rest>`. */
const WORKSPACE_STORAGE_PREFIXES = [
	"dcc.workspace.composer.draft.",
	"dcc.workspace.composer.effort.",
	"dcc.workspace.composer.approval.",
];

/** Written by code that no longer exists; nothing reads them. */
const LEGACY_STORAGE_KEY_PREFIXES = ["dcc:attention:records", "dcc-review-trail-"];

function storageKeys(storage: Storage): string[] {
	const keys: string[] = [];
	for (let index = 0; index < storage.length; index += 1) {
		const key = storage.key(index);
		if (key) keys.push(key);
	}
	return keys;
}

function browserStorage(): Storage | null {
	try {
		return typeof window === "undefined" ? null : window.localStorage;
	} catch {
		return null;
	}
}

/** A deleted task must not leave its drafts and composer choices behind. */
export function removeDeletedWorkspaceStorage(
	storage: Storage,
	input: { workspaceIds: Iterable<string>; sessionIds: Iterable<string> },
) {
	const workspaceIds = [...input.workspaceIds].filter(Boolean);
	for (const key of storageKeys(storage)) {
		const prefix = WORKSPACE_STORAGE_PREFIXES.find((candidate) => key.startsWith(candidate));
		if (!prefix) continue;
		const rest = key.slice(prefix.length);
		if (workspaceIds.some((id) => rest === id || rest.startsWith(`${id}.`))) {
			storage.removeItem(key);
		}
	}

	const sessionIds = [...input.sessionIds];
	const raw = storage.getItem(SESSION_COMPOSER_MAP_KEY);
	if (!raw || sessionIds.length === 0) return;
	try {
		const map = JSON.parse(raw) as Record<string, unknown>;
		const before = Object.keys(map).length;
		for (const sessionId of sessionIds) delete map[sessionId];
		if (Object.keys(map).length !== before) {
			storage.setItem(SESSION_COMPOSER_MAP_KEY, JSON.stringify(map));
		}
	} catch {
		/* A malformed map is reset by its own reader. */
	}
}

export function removeLegacyStorage(storage: Storage) {
	for (const key of storageKeys(storage)) {
		if (LEGACY_STORAGE_KEY_PREFIXES.some((prefix) => key.startsWith(prefix))) {
			storage.removeItem(key);
		}
	}
}

export function removeLegacyBrowserStorage() {
	const storage = browserStorage();
	if (!storage) return;
	try {
		removeLegacyStorage(storage);
	} catch {
		/* Storage unavailable; nothing to clean. */
	}
}

const HEAVY_QUERY_ROOTS = new Set([
	"workspaceFileContent",
	"workspaceGitFilePreviewContent",
	"workspaceGitBranchDiff",
	"workspacePipelineJobLog",
	"workspaceSearch",
	"multiWorkspaceChanges",
	"workspaceGitStatus",
	"workspacePrStatus",
	"workspacePipeline",
	"workspaceDeliveryFailureSnapshot",
	"workspaceReviewState",
	"workspaceReviewerCandidates",
]);

function underRoot(value: unknown, roots: readonly string[]) {
	if (typeof value !== "string") return false;
	const normalizedValue = value.replace(/\\/g, "/").replace(/\/+$/, "");
	return roots.some((root) => {
		const normalized = root.replace(/\\/g, "/").replace(/\/+$/, "");
		return (
			normalizedValue === normalized ||
			normalizedValue.startsWith(`${normalized}/`)
		);
	});
}

export function removeSessionFrontendQueries(
	queryClient: QueryClient,
	sessionIds: Iterable<string>,
) {
	const ids = new Set(sessionIds);
	if (ids.size === 0) return;
	queryClient.removeQueries({
		predicate: (query) =>
			query.queryKey[0] === "sessionThreads" && ids.has(String(query.queryKey[2])),
	});
}

export function removeWorkspaceHeavyQueries(
	queryClient: QueryClient,
	input: { workspaceIds?: Iterable<string>; roots?: Iterable<string> },
) {
	const workspaceIds = new Set(input.workspaceIds ?? []);
	const roots = [...(input.roots ?? [])].filter(Boolean);
	queryClient.removeQueries({
		predicate: (query) => {
			const queryRoot = query.queryKey[0];
			if (typeof queryRoot !== "string" || !HEAVY_QUERY_ROOTS.has(queryRoot)) return false;
			return query.queryKey.some(
				(value) => workspaceIds.has(String(value)) || underRoot(value, roots),
			);
		},
	});
}

/** Completion keeps summaries/history durable; it only sheds reloadable payloads. */
export function cleanupCompletedWorkspaceFrontendState(
	queryClient: QueryClient,
	input: { workspaceIds: Iterable<string>; roots: Iterable<string> },
) {
	removeWorkspaceHeavyQueries(queryClient, input);
}

export function cleanupDeletedWorkspaceFrontendState(
	queryClient: QueryClient,
	input: {
		workspaceIds: Iterable<string>;
		sessionIds: Iterable<string>;
		roots: Iterable<string>;
	},
) {
	const workspaceIds = new Set(input.workspaceIds);
	const sessionIds = [...input.sessionIds];
	removeSessionFrontendQueries(queryClient, sessionIds);
	removeWorkspaceHeavyQueries(queryClient, { workspaceIds, roots: input.roots });
	queryClient.removeQueries({
		predicate: (query) =>
			query.queryKey[0] === "workspaceSessions" &&
			workspaceIds.has(String(query.queryKey[2])),
	});
	const storage = browserStorage();
	if (!storage) return;
	try {
		removeDeletedWorkspaceStorage(storage, { workspaceIds, sessionIds });
	} catch {
		/* Storage unavailable; the task is already gone. */
	}
}
