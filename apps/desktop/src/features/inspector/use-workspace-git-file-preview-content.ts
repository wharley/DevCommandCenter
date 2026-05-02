import { useQuery } from "@tanstack/react-query";
import { workspaceGitFilePreviewContent } from "@/lib/workspace-api";
import type { WorkspaceGitFilePreviewInput } from "@dcc/contracts";

export const WORKSPACE_GIT_FILE_PREVIEW_CONTENT_QUERY_KEY =
	"workspaceGitFilePreviewContent";

export function useWorkspaceGitFilePreviewContent(
	input: WorkspaceGitFilePreviewInput | null,
) {
	const root = input?.workspaceRoot.trim() ?? "";
	const relativePath = input?.relativePath.trim() ?? "";
	const scope = input?.scope ?? null;
	const status = input?.status.trim() ?? "";
	const baseBranch = input?.baseBranch?.trim() ?? "";

	return useQuery({
		queryKey: [
			WORKSPACE_GIT_FILE_PREVIEW_CONTENT_QUERY_KEY,
			root,
			relativePath,
			scope,
			status,
			baseBranch,
		],
		queryFn: async () => {
			if (!input || !root || !relativePath || !scope) {
				return {
					originalText: "",
					modifiedText: "",
					inline: false,
				};
			}

			return workspaceGitFilePreviewContent({
				...input,
				workspaceRoot: root,
				relativePath,
				status,
				baseBranch: baseBranch.length > 0 ? baseBranch : null,
			});
		},
		enabled: Boolean(input && root && relativePath && scope),
		staleTime: 15_000,
		refetchOnWindowFocus: false,
	});
}
