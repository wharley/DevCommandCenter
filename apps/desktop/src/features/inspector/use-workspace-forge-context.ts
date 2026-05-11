import { useQuery } from "@tanstack/react-query";
import type { WorkspaceForgeContextOutput } from "@dcc/contracts";
import { workspaceForgeContext } from "@/lib/workspace-api";

export const WORKSPACE_FORGE_CONTEXT_QUERY_KEY = "workspaceForgeContext";

const EMPTY_WORKSPACE_FORGE_CONTEXT: WorkspaceForgeContextOutput = {
	provider: null,
	host: null,
	remoteName: null,
	namespace: null,
	repo: null,
	cliName: null,
	status: null,
	remoteState: null,
	boundLogin: null,
	login: null,
	selectedLogin: null,
	effectiveLogin: null,
	knownHosts: [],
	message: null,
	loginCommand: null,
};

function normalizeWorkspaceForgeContext(
	context: Partial<WorkspaceForgeContextOutput> | null | undefined,
): WorkspaceForgeContextOutput {
	return {
		...EMPTY_WORKSPACE_FORGE_CONTEXT,
		...context,
		knownHosts: Array.isArray(context?.knownHosts) ? context.knownHosts : [],
	};
}

export function useWorkspaceForgeContext(
	workspaceRoot: string | null,
	forgeLogin: string | null = null,
) {
	const root = workspaceRoot?.trim() ?? "";
	const selectedLogin = forgeLogin?.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY, root, selectedLogin],
		queryFn: async () => {
			if (!root) {
				return EMPTY_WORKSPACE_FORGE_CONTEXT;
			}

			const context = await workspaceForgeContext({
				workspaceRoot: root,
				forgeLogin: selectedLogin.length > 0 ? selectedLogin : null,
			});

			return normalizeWorkspaceForgeContext(context);
		},
		enabled: Boolean(root),
		staleTime: 15_000,
		refetchOnWindowFocus: true,
	});
}
