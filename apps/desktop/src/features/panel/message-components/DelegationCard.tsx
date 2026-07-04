import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { Delegation, ProviderCatalog } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { listDelegations } from "@/lib/delegation-api";
import { delegationStatusClass } from "@/features/sessions/delegation-status";
import type { WorkspaceMessageDelegation } from "@/features/sessions/session-thread-history.logic";
import { MessageTimestamp } from "./message-metadata";

const PHASE_STATUS: Record<WorkspaceMessageDelegation["phase"], Delegation["status"]> = {
	requested: "queued",
	running: "running",
	completed: "completed",
	failed: "failed",
	cancelled: "cancelled",
};

export function DelegationCard({
	delegation,
	fallbackContent,
	createdAt,
	workspaceId,
	providers,
	onSelectSession,
	onReviewChanges,
	onReviewDelegation,
}: {
	delegation: WorkspaceMessageDelegation;
	fallbackContent: string;
	createdAt?: string;
	workspaceId: string | null;
	providers: ProviderCatalog["providers"];
	onSelectSession: (sessionId: string) => void;
	onReviewChanges?: () => void;
	onReviewDelegation?: (delegationId: string) => void;
}) {
	const { t } = useTranslation("common");
	// Shares queryKey and options with the Inspector's Delegations section so
	// both surfaces read the same cache entry and poll only once.
	const delegationsQuery = useQuery({
		queryKey: ["delegations", workspaceId],
		queryFn: async () => {
			if (!workspaceId) {
				return [] as Delegation[];
			}
			const output = await listDelegations({
				workspaceId,
				parentSessionId: null,
			});
			return output.delegations;
		},
		enabled: Boolean(workspaceId),
		staleTime: 5_000,
		refetchInterval: 10_000,
	});
	const record =
		delegationsQuery.data?.find((item) => item.id === delegation.id) ?? null;

	const status: Delegation["status"] = record?.status ?? PHASE_STATUS[delegation.phase];
	const statusLabel = t(`inspector.delegations.status.${status}`, {
		defaultValue: status,
	});
	const modeLabel = record
		? t(`inspector.delegations.mode.${record.mode}`, { defaultValue: record.mode })
		: null;
	const providerLabel = record
		? (providers.find((provider) => provider.id === record.targetProviderId)?.label ??
			record.targetProviderId)
		: null;
	const summary =
		record?.resultSummary ??
		delegation.summary ??
		delegation.reason ??
		(fallbackContent || record?.prompt) ??
		"";
	const touchedFiles = record?.touchedFiles ?? [];
	const childSessionId = record?.childSessionId ?? delegation.childSessionId ?? null;
	const showReview =
		Boolean(onReviewDelegation || onReviewChanges) &&
		(status === "review_pending" || touchedFiles.length > 0);
	const handleReview = () => {
		if (onReviewDelegation) {
			onReviewDelegation(delegation.id);
			return;
		}
		onReviewChanges?.();
	};

	return (
		<div
			data-message-role="system"
			className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-center px-4"
		>
			<div className="w-full max-w-[42rem] rounded-xl border border-border/70 bg-card/70 px-4 py-3">
				<div className="flex items-center justify-between gap-3">
					<div className="flex min-w-0 items-center gap-2">
						<p className="shrink-0 text-[10px] font-medium uppercase tracking-[0.18em] text-muted-foreground/80">
							{t("delegation.card.title")}
						</p>
						{modeLabel || providerLabel ? (
							<p className="min-w-0 truncate text-[12px] font-medium text-foreground">
								{[modeLabel, providerLabel].filter(Boolean).join(" · ")}
							</p>
						) : null}
					</div>
					<Badge
						variant="outline"
						className={cn(
							"h-5 shrink-0 px-1.5 text-[10px] font-medium",
							delegationStatusClass(status),
						)}
					>
						{status === "running" ? (
							<span
								aria-hidden
								className="mr-1 inline-block size-1.5 animate-pulse rounded-full bg-current"
							/>
						) : null}
						{statusLabel}
					</Badge>
				</div>

				{summary ? (
					<p className="mt-2 whitespace-pre-wrap break-words text-[12px] leading-5 text-muted-foreground">
						{summary}
					</p>
				) : null}

				{touchedFiles.length > 0 ? (
					<p className="mt-1.5 truncate font-mono text-[10.5px] text-muted-foreground/80">
						{t("inspector.delegations.files", { count: touchedFiles.length })}:{" "}
						{touchedFiles.slice(0, 3).join(", ")}
					</p>
				) : null}

				<div className="mt-2.5 flex flex-wrap items-center gap-1.5">
					{childSessionId ? (
						<Button
							type="button"
							variant="outline"
							size="sm"
							className="h-6 px-2 text-[11px]"
							onClick={() => onSelectSession(childSessionId)}
						>
							{t("delegation.card.openChild")}
						</Button>
					) : null}
					{showReview ? (
						<Button
							type="button"
							variant={status === "review_pending" ? "default" : "ghost"}
							size="sm"
							className="h-6 px-2 text-[11px]"
							onClick={handleReview}
						>
							{t("delegation.card.reviewInInspector")}
						</Button>
					) : null}
					<span className="ml-auto inline-flex items-center text-[11px] leading-none text-muted-foreground/60">
						<MessageTimestamp createdAt={createdAt} />
					</span>
				</div>
			</div>
		</div>
	);
}
