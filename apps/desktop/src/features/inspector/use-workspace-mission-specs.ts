import { useQuery } from "@tanstack/react-query";
import { listMissionSpecs } from "@/lib/workspace-api";

export const WORKSPACE_MISSION_SPECS_QUERY_KEY = "workspaceMissionSpecs";

export function useWorkspaceMissionSpecs(workspaceRoot: string | null) {
	const root = workspaceRoot?.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_MISSION_SPECS_QUERY_KEY, root],
		queryFn: async () => {
			if (!root) {
				return { specs: [] };
			}
			return listMissionSpecs({ workspaceRoot: root });
		},
		enabled: Boolean(root),
		staleTime: 5_000,
		refetchOnWindowFocus: true,
	});
}
