import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef } from "react";
import type { ForgeCliProvider } from "@dcc/contracts";
import { getForgeCliAccounts, normalizeForgeHost } from "@/lib/forge-cli";
import {
	forgeCliAccountsQueryKey,
	forgeCliStatusQueryKey,
} from "@/features/settings/forge-cli-queries";
import { WORKSPACE_FORGE_CONTEXT_QUERY_KEY } from "@/features/inspector/use-workspace-forge-context";
import { WORKSPACE_PR_STATUS_QUERY_KEY } from "@/features/inspector/use-workspace-pr-status";

function setsDiffer(a: Set<string>, b: Set<string>): boolean {
	if (a.size !== b.size) {
		return true;
	}
	for (const value of a) {
		if (!b.has(value)) {
			return true;
		}
	}
	return false;
}

export function useForgeCliLoginsHealth(
	provider: ForgeCliProvider,
	host?: string | null,
	options?: { enabled?: boolean },
) {
	const queryClient = useQueryClient();
	const previousRef = useRef<Set<string> | null>(null);
	const normalizedHost = normalizeForgeHost(provider, host);

	return useQuery({
		queryKey: ["forgeCliLoginsHealth", provider, normalizedHost],
		queryFn: async () => {
			const accounts = await getForgeCliAccounts(provider, normalizedHost, {
				forceRefresh: true,
			});
			const nextSet = new Set(accounts.accounts.map((account) => account.login));
			const previousSet = previousRef.current;
			previousRef.current = nextSet;

			if (previousSet && setsDiffer(previousSet, nextSet)) {
				void Promise.all([
					queryClient.invalidateQueries({
						queryKey: forgeCliStatusQueryKey(provider, normalizedHost),
					}),
					queryClient.invalidateQueries({
						queryKey: forgeCliAccountsQueryKey(provider, normalizedHost),
					}),
					queryClient.invalidateQueries({
						predicate: (query) => {
							const head = query.queryKey[0];
							return (
								head === WORKSPACE_FORGE_CONTEXT_QUERY_KEY ||
								head === WORKSPACE_PR_STATUS_QUERY_KEY
							);
						},
					}),
				]);
			}

			return [...nextSet];
		},
		staleTime: 0,
		refetchOnWindowFocus: "always",
		retry: 0,
		enabled: options?.enabled,
	});
}
