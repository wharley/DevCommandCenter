import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { LoaderCircle } from "lucide-react";
import type { Delegation, ProviderCatalog } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { cancelDelegation, startTaskProposal } from "@/lib/delegation-api";
import { describeDelegation } from "@/features/sessions/delegation-decisions";
import { delegationInstruction } from "@/features/sessions/delegation-lineage";
import { AgentMascot } from "@/features/providers/agent-mascot";
import { AssistantProse } from "./AssistantProse";
import { MessageTimestamp } from "./message-metadata";

/**
 * A task the parent agent proposed with `dcc_propose_task`. Nothing runs until
 * the person starts it; the brief can be edited first. Once started, the same
 * record becomes an ordinary task card through `DelegationCard`.
 */
export function TaskProposalCard({
	proposal,
	workspaceId,
	providers,
	createdAt,
}: {
	proposal: Delegation;
	workspaceId: string | null;
	providers: ProviderCatalog["providers"];
	createdAt?: string;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const brief = delegationInstruction(proposal);
	const [draft, setDraft] = useState<string | null>(null);
	const [pending, setPending] = useState<"start" | "dismiss" | null>(null);
	const [error, setError] = useState<string | null>(null);

	const decisions = describeDelegation(proposal, providers);
	const editsFiles = proposal.mode === "implement" || proposal.budget.allowFileEdits;
	const meta = [
		t("delegation.proposal.label"),
		decisions.modelLabel ?? decisions.providerLabel,
		editsFiles ? t("delegation.proposal.worktree") : t("delegation.proposal.readOnly"),
	].join(" · ");

	const act = async (action: "start" | "dismiss") => {
		if (pending) {
			return;
		}
		setPending(action);
		setError(null);
		try {
			if (action === "start") {
				const edited = draft?.trim();
				await startTaskProposal({
					delegationId: proposal.id,
					instruction: edited && edited !== brief.trim() ? edited : null,
				});
			} else {
				await cancelDelegation({
					delegationId: proposal.id,
					reason: "Dismissed by the person.",
				});
			}
			await queryClient.invalidateQueries({ queryKey: ["delegations", workspaceId] });
		} catch (cause) {
			setError(cause instanceof Error ? cause.message : String(cause));
		} finally {
			setPending(null);
		}
	};

	return (
		<div
			data-message-role="system"
			className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-center px-4"
		>
			<div className="w-full max-w-[42rem] rounded-xl border border-dashed border-border bg-card/70 px-4 py-3">
				<div className="flex items-start gap-2.5">
					<AgentMascot
						provider={proposal.targetProviderId}
						model={proposal.targetModelId ?? null}
						state="idle"
						className="mt-0.5"
					/>
					<div className="min-w-0 flex-1">
						<p
							className="truncate text-[13px] font-medium leading-5 text-foreground"
							title={proposal.title ?? undefined}
						>
							{proposal.title || t("delegation.proposal.label")}
						</p>
						<p className="truncate text-[11px] leading-4 text-muted-foreground">{meta}</p>
					</div>
				</div>

				{draft === null ? (
					<div className="mt-2 max-h-40 overflow-auto text-[12.5px]">
						<AssistantProse content={brief} />
					</div>
				) : (
					<Textarea
						value={draft}
						onChange={(event) => setDraft(event.target.value)}
						className="mt-2 min-h-28 text-[12.5px]"
						autoFocus
					/>
				)}

				<p className="mt-2 text-[11px] leading-4 text-muted-foreground">
					{t("delegation.proposal.hint")}
				</p>
				{error ? (
					<p className="mt-1.5 text-[11.5px] leading-4 text-destructive">{error}</p>
				) : null}

				<div className="mt-3 flex flex-wrap items-center gap-1.5">
					<Button
						type="button"
						size="sm"
						className="h-7 gap-1 px-2.5 text-[11.5px]"
						disabled={pending !== null || (draft !== null && !draft.trim())}
						onClick={() => void act("start")}
					>
						{pending === "start" ? (
							<LoaderCircle className="size-3 animate-spin" aria-hidden />
						) : null}
						{pending === "start" ? t("delegation.proposal.starting") : t("delegation.proposal.start")}
					</Button>
					{draft === null ? (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="h-7 px-2 text-[11.5px]"
							disabled={pending !== null}
							onClick={() => setDraft(brief)}
						>
							{t("delegation.proposal.editBrief")}
						</Button>
					) : null}
					<Button
						type="button"
						variant="ghost"
						size="sm"
						className="h-7 px-2 text-[11.5px] text-muted-foreground"
						disabled={pending !== null}
						onClick={() => void act("dismiss")}
					>
						{t("delegation.proposal.dismiss")}
					</Button>
					<span className="ml-auto inline-flex items-center text-[11px] leading-none text-muted-foreground/60">
						<MessageTimestamp createdAt={createdAt} />
					</span>
				</div>
			</div>
		</div>
	);
}
