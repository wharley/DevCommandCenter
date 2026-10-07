import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderCatalog } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { AgentMascot } from "@/features/providers/agent-mascot";
import { describeDelegation } from "@/features/sessions/delegation-decisions";
import { useWorkspaceDelegations } from "@/features/sessions/use-workspace-delegations";

const MAX_STRIPS = 3;

/**
 * The next action after a delegation, where the reading ends: the edits a
 * child agent left in its worktree for this conversation, with the way to
 * review them. The card higher up may already be scrolled out of sight.
 */
export function DelegationReviewStrip({
	workspaceId,
	sessionId,
	providers,
	onReviewDelegation,
}: {
	workspaceId: string | null;
	sessionId: string | null;
	providers: ProviderCatalog["providers"];
	onReviewDelegation: (delegationId: string) => void;
}) {
	const { t } = useTranslation("common");
	const { data: delegations = [] } = useWorkspaceDelegations(workspaceId);
	const pending = useMemo(
		() =>
			delegations
				.filter(
					(delegation) =>
						delegation.parentSessionId === sessionId &&
						delegation.mode === "implement" &&
						delegation.status === "review_pending",
				)
				.slice(0, MAX_STRIPS),
		[delegations, sessionId],
	);
	if (!sessionId || pending.length === 0) return null;

	return (
		<div className="flex flex-col gap-1.5 px-4 pb-4">
			{pending.map((delegation) => {
				const decisions = describeDelegation(delegation, providers);
				const agent = decisions.modelLabel ?? decisions.providerLabel;
				return (
					<div
						key={delegation.id}
						className="mx-auto flex w-full max-w-[42rem] items-center gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2"
					>
						<AgentMascot
							provider={delegation.targetProviderId}
							model={delegation.targetModelId ?? null}
							state="waiting"
						/>
						<p className="min-w-0 flex-1 truncate text-[12px] font-medium text-amber-800 dark:text-amber-200">
							{t("delegation.reviewStrip.message", {
								agent,
								count: delegation.touchedFiles?.length ?? 0,
							})}
						</p>
						<Button
							type="button"
							size="sm"
							className="h-7 shrink-0 px-3 text-[11.5px]"
							onClick={() => onReviewDelegation(delegation.id)}
						>
							{t("delegation.reviewStrip.action")}
						</Button>
					</div>
				);
			})}
		</div>
	);
}
