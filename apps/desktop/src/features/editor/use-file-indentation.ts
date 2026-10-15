import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { readWorkspaceFile } from "@/lib/workspace-api";
import { resolveFileIndentation, type FileIndentation } from "./indentation";

/**
 * The indentation to type with in a worktree file: the project's
 * `.editorconfig` first, then what the file already does.
 */
export function useFileIndentation(
	workspaceRoot: string | null,
	relativePath: string,
	content: string,
): FileIndentation {
	const root = workspaceRoot?.trim() ?? "";
	const editorConfigQuery = useQuery({
		queryKey: ["workspaceEditorConfig", root],
		queryFn: async () => {
			try {
				const result = await readWorkspaceFile({
					workspaceRoot: root,
					relativePath: ".editorconfig",
				});
				return result.content;
			} catch {
				// Most projects have none; the file's own style decides.
				return null;
			}
		},
		enabled: Boolean(root),
		staleTime: 60_000,
		refetchOnWindowFocus: false,
	});
	const editorConfig = editorConfigQuery.data ?? null;
	return useMemo(
		() => resolveFileIndentation(editorConfig, relativePath, content),
		[content, editorConfig, relativePath],
	);
}
