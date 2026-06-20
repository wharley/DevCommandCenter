import { useQuery, type QueryClient } from "@tanstack/react-query";
import { getCodeRabbitCliStatus } from "@/lib/coderabbit-cli";

export function codeRabbitCliStatusQueryKey(workspaceRoot?: string | null) {
	return ["codeRabbitCliStatus", workspaceRoot?.trim() ?? "global"] as const;
}

export function useCodeRabbitCliStatus(
	workspaceRoot?: string | null,
	options?: { enabled?: boolean; includeAuthStatus?: boolean },
) {
	const root = workspaceRoot?.trim() || null;
	return useQuery({
		queryKey: codeRabbitCliStatusQueryKey(root),
		queryFn: () =>
			getCodeRabbitCliStatus({
				workspaceRoot: root,
				includeAuthStatus: options?.includeAuthStatus ?? true,
			}),
		staleTime: 60_000,
		refetchOnWindowFocus: true,
		enabled: options?.enabled,
	});
}

export async function invalidateCodeRabbitCliQueries(
	queryClient: QueryClient,
	workspaceRoot?: string | null,
) {
	await Promise.all([
		queryClient.invalidateQueries({
			queryKey: codeRabbitCliStatusQueryKey(workspaceRoot?.trim() || null),
		}),
		queryClient.invalidateQueries({
			queryKey: codeRabbitCliStatusQueryKey(null),
		}),
	]);
}
