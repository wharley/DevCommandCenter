import { useQuery } from "@tanstack/react-query";
import { workspaceGitFilePreviewContent } from "@/lib/workspace-api";

export const WORKSPACE_FILE_CONTENT_QUERY_KEY = "workspaceFileContent";

/**
 * Reads the current working-tree body of an arbitrary worktree file (e.g. one
 * opened from Quick Open, which may not be a pending change).
 *
 * Stopgap: it borrows the diff preview command in `unstaged` scope — that path
 * returns `HEAD` vs working tree, and we keep the working-tree side. This works
 * for any tracked file regardless of whether it has pending changes. When a
 * dedicated `read_workspace_file` command lands (see docs/PLANO_DCC_CODE.md), only
 * this `queryFn` changes; callers stay the same.
 */
export function useWorkspaceFileContent(
	input: { workspaceRoot: string; relativePath: string } | null,
) {
	const root = input?.workspaceRoot.trim() ?? "";
	const relativePath = input?.relativePath.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_FILE_CONTENT_QUERY_KEY, root, relativePath],
		queryFn: async () => {
			const snapshot = await workspaceGitFilePreviewContent({
				workspaceRoot: root,
				relativePath,
				status: "M",
				scope: "unstaged",
				baseBranch: null,
			});
			return { content: snapshot.modifiedText || snapshot.originalText };
		},
		enabled: Boolean(root && relativePath),
		staleTime: 15_000,
		refetchOnWindowFocus: false,
	});
}
