import { queryOptions } from "@tanstack/react-query";
import { listChildDirectories } from "@/lib/workspace-api";
import { pathBasename } from "@/lib/path-basename";

export type WorkspaceChildDirCandidate = {
	absolutePath: string;
	title: string;
};

export function workspaceChildDirsQueryOptions(workspaceRoot: string | null) {
	return queryOptions({
		queryKey: ["workspace-child-dirs", workspaceRoot] as const,
		queryFn: async (): Promise<WorkspaceChildDirCandidate[]> => {
			if (!workspaceRoot) {
				return [];
			}
			const { paths } = await listChildDirectories({ path: workspaceRoot });
			return paths.map((absolutePath) => ({
				absolutePath,
				title: pathBasename(absolutePath),
			}));
		},
		enabled: Boolean(workspaceRoot),
		staleTime: 30_000,
	});
}
