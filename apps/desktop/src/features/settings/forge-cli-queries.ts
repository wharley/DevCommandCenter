import { useQuery, type QueryClient } from "@tanstack/react-query";
import type { ForgeCliProvider } from "@dcc/contracts";
import {
	getForgeCliAccounts,
	getForgeCliHosts,
	getForgeCliStatus,
	normalizeForgeHost,
} from "@/lib/forge-cli";

export function forgeCliStatusQueryKey(provider: ForgeCliProvider, host?: string | null) {
	return ["forgeCliStatus", provider, normalizeForgeHost(provider, host)] as const;
}

export function forgeCliAccountsQueryKey(provider: ForgeCliProvider, host?: string | null) {
	return ["forgeCliAccounts", provider, normalizeForgeHost(provider, host)] as const;
}

export function forgeCliHostsQueryKey(provider: ForgeCliProvider) {
	return ["forgeCliHosts", provider] as const;
}

export function useForgeCliStatus(
	provider: ForgeCliProvider,
	host?: string | null,
	options?: { enabled?: boolean },
) {
	const normalizedHost = normalizeForgeHost(provider, host);
	return useQuery({
		queryKey: forgeCliStatusQueryKey(provider, normalizedHost),
		queryFn: () => getForgeCliStatus(provider, normalizedHost),
		staleTime: 60_000,
		refetchOnWindowFocus: true,
		enabled: options?.enabled,
	});
}

export function useForgeCliAccounts(
	provider: ForgeCliProvider,
	host?: string | null,
	options?: { enabled?: boolean },
) {
	const normalizedHost = normalizeForgeHost(provider, host);
	return useQuery({
		queryKey: forgeCliAccountsQueryKey(provider, normalizedHost),
		queryFn: () => getForgeCliAccounts(provider, normalizedHost),
		staleTime: 60_000,
		refetchOnWindowFocus: true,
		enabled: options?.enabled,
	});
}

export function useForgeCliHosts(
	provider: ForgeCliProvider,
	options?: { enabled?: boolean },
) {
	return useQuery({
		queryKey: forgeCliHostsQueryKey(provider),
		queryFn: () => getForgeCliHosts(provider),
		staleTime: 60_000,
		refetchOnWindowFocus: true,
		enabled: options?.enabled,
	});
}

export async function invalidateForgeCliQueries(
	queryClient: QueryClient,
	provider: ForgeCliProvider,
	host?: string | null,
) {
	await Promise.all([
		queryClient.invalidateQueries({
			queryKey: forgeCliStatusQueryKey(provider, host),
		}),
		queryClient.invalidateQueries({
			queryKey: forgeCliAccountsQueryKey(provider, host),
		}),
		queryClient.invalidateQueries({
			queryKey: forgeCliHostsQueryKey(provider),
		}),
	]);
}
