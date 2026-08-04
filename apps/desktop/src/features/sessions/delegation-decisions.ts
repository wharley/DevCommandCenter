import type { Delegation, ProviderCatalog } from "@dcc/contracts";

/**
 * The decisions the DCC took on the user's behalf when a delegation started.
 *
 * They are shown on the delegation card as the *result* of a run rather than
 * asked for up front — this is the surface where `contextPolicy` legitimately
 * exists as a concept, because there is now a concrete artifact to attach it to.
 */
export type DelegationDecisions = {
	mode: Delegation["mode"];
	providerId: string;
	providerLabel: string;
	modelId: string | null;
	modelLabel: string | null;
	contextPolicy: Delegation["contextPolicy"]["type"];
	allowFileEdits: boolean;
};

export function describeDelegation(
	record: Delegation,
	providers: ProviderCatalog["providers"],
): DelegationDecisions {
	const provider = providers.find(
		(candidate) => candidate.id === record.targetProviderId,
	);
	const model = provider?.models?.find(
		(candidate) => candidate.id === record.targetModelId,
	);
	return {
		mode: record.mode,
		providerId: record.targetProviderId,
		providerLabel: provider?.label ?? record.targetProviderId,
		modelId: record.targetModelId ?? null,
		modelLabel: model?.label ?? record.targetModelId ?? null,
		contextPolicy: record.contextPolicy.type,
		allowFileEdits: record.budget.allowFileEdits,
	};
}

const RERUNNABLE_STATUSES: ReadonlySet<Delegation["status"]> = new Set([
	"completed",
	"failed",
	"cancelled",
]);

/**
 * A rerun replays the stored prompt verbatim on another agent, so it is only
 * offered when that prompt still describes a reachable workspace.
 *
 * Implementation prompts pin the child worktree path, and that worktree is
 * removed once the delegation is applied or discarded — replaying one would
 * point the new child at a path that no longer exists.
 */
export function canRerunDelegation(
	record: Delegation | null | undefined,
): record is Delegation {
	if (!record) {
		return false;
	}
	if (record.mode === "implement" || record.budget.allowFileEdits) {
		return false;
	}
	return RERUNNABLE_STATUSES.has(record.status);
}

/**
 * Reruns are read-only by construction, so the wider domain modes collapse onto
 * the two the manual delegation request accepts.
 */
export function rerunMode(record: Delegation): "review" | "explain" {
	return record.mode === "explain" ? "explain" : "review";
}

/** Rerun targets exclude the agent that already ran this delegation. */
export function rerunTargets(
	record: Delegation,
	targets: ProviderCatalog["providers"],
) {
	return targets.filter((target) => target.id !== record.targetProviderId);
}
