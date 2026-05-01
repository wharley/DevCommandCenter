import { queryOptions } from "@tanstack/react-query";
import { listGitTrackedFiles } from "@/lib/workspace-api";
import { pathBasename } from "@/lib/path-basename";

export type TrackedComposerFile = {
	path: string;
	name: string;
};

export function workspaceTrackedFilesQueryOptions(workspaceRoot: string | null) {
	return queryOptions({
		queryKey: ["workspace-git-files", workspaceRoot] as const,
		queryFn: async (): Promise<TrackedComposerFile[]> => {
			if (!workspaceRoot) {
				return [];
			}
			const { paths } = await listGitTrackedFiles({ workspaceRoot });
			return paths.map((path) => ({
				path,
				name: pathBasename(path),
			}));
		},
		enabled: Boolean(workspaceRoot),
		staleTime: 60_000,
	});
}
