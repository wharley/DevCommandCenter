import { useQuery } from "@tanstack/react-query";
import { workspacePipelineStatus } from "@/lib/workspace-api";

export const WORKSPACE_PIPELINE_QUERY_KEY = "workspacePipeline";
const WORKSPACE_PIPELINE_REFETCH_INTERVAL_MS = 15_000;

export function useWorkspacePipeline(
	workspaceRoot: string | null,
	forgeLogin: string | null = null,
	enabled = true,
) {
	const root = workspaceRoot?.trim() ?? "";
	const selectedLogin = forgeLogin?.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_PIPELINE_QUERY_KEY, root, selectedLogin],
		queryFn: () =>
			workspacePipelineStatus({
				workspaceRoot: root,
				forgeLogin: selectedLogin || null,
			}),
		enabled: Boolean(enabled && root),
		staleTime: 10_000,
		refetchOnWindowFocus: true,
		refetchInterval: WORKSPACE_PIPELINE_REFETCH_INTERVAL_MS,
	});
}
