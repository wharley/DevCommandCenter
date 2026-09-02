import type { ProviderCatalog } from "@dcc/contracts";
import { isProviderEnabled } from "@/features/providers/provider-selection.logic";

export type DelegationTarget = ProviderCatalog["providers"][number];

/**
 * Providers the DCC can hand scoped work to. Read-only support is the floor:
 * every delegation mode at minimum reads the parent context.
 */
export function eligibleDelegationTargets(
	providers: ProviderCatalog["providers"],
): DelegationTarget[] {
	return providers.filter(
		(provider) =>
			isProviderEnabled(provider) &&
			provider.capabilities.canBeDelegationTarget &&
			provider.capabilities.supportsReadOnlyDelegation,
	);
}

/** Edit delegations run in an isolated worktree, so the target must support edits. */
export function canDelegateEdits(target: DelegationTarget | null | undefined) {
	return Boolean(target?.capabilities.supportsEditDelegation);
}

/** Targets offered for a given permission level; edits narrow the list. */
export function delegationTargetsFor(
	providers: ProviderCatalog["providers"],
	options: { allowFileEdits: boolean },
): DelegationTarget[] {
	const targets = eligibleDelegationTargets(providers);
	return options.allowFileEdits ? targets.filter(canDelegateEdits) : targets;
}
