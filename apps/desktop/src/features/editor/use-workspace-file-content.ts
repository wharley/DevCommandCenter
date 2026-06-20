import { useQuery } from "@tanstack/react-query";
import { readWorkspaceFile } from "@/lib/workspace-api";

export const WORKSPACE_FILE_CONTENT_QUERY_KEY = "workspaceFileContent";

/**
 * Reads the current working-tree body of an arbitrary worktree file (e.g. one
 * opened from Quick Open, which may not be a pending change). Backed by the
 * `read_workspace_file` command, which confines the path to the workspace root.
 */
export function useWorkspaceFileContent(
	input: { workspaceRoot: string; relativePath: string } | null,
) {
	const root = input?.workspaceRoot.trim() ?? "";
	const relativePath = input?.relativePath.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_FILE_CONTENT_QUERY_KEY, root, relativePath],
		queryFn: () =>
			readWorkspaceFile({ workspaceRoot: root, relativePath }),
		enabled: Boolean(root && relativePath),
		staleTime: 15_000,
		refetchOnWindowFocus: false,
	});
}
