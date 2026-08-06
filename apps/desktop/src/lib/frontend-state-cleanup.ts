import type { QueryClient } from "@tanstack/react-query";

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
	removeSessionFrontendQueries(queryClient, input.sessionIds);
	removeWorkspaceHeavyQueries(queryClient, { workspaceIds, roots: input.roots });
	queryClient.removeQueries({
		predicate: (query) =>
			query.queryKey[0] === "workspaceSessions" &&
			workspaceIds.has(String(query.queryKey[2])),
	});
}
