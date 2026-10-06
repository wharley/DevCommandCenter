import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { DCC_QUERY_GC_TIME_MS } from "@/lib/query-client";
import { workspaceCleanupScan } from "@/lib/workspace-api";
import type { WorkspaceSummary } from "./types";
import { workspaceDiskUsageIds } from "./workspace-disk-usage";

/** Completed tasks rarely change on disk; a new set of them is what triggers a new pass. */
const COMPLETED_CLEANUP_STALE_MS = 10 * 60_000;

/**
 * Size and delete-safety of the completed tasks' worktrees, measured once per
 * set of tasks rather than on every sidebar render. The cleanup list scans
 * again on its own before deleting anything.
 */
export function useCompletedCleanupScan(
	completedRows: readonly WorkspaceSummary[],
	enabled: boolean,
) {
	const workspaceIds = useMemo(
		() => workspaceDiskUsageIds(completedRows).sort(),
		[completedRows],
	);
	return useQuery({
		queryKey: ["completedCleanupScan", workspaceIds.join("\n")],
		queryFn: () => workspaceCleanupScan({ workspaceIds }),
		enabled: enabled && workspaceIds.length > 0,
		staleTime: COMPLETED_CLEANUP_STALE_MS,
		gcTime: DCC_QUERY_GC_TIME_MS.default,
		placeholderData: keepPreviousData,
	});
}
