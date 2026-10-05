import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { CornerLeftUp, GitFork, LoaderCircle, Square } from "lucide-react";
import { toast } from "sonner";
import type { Delegation, WorkspaceSessionSummary } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { ProviderIcon } from "@/features/providers/provider-icons";
import { cancelRunningDelegation } from "@/lib/delegation-api";
import { cn } from "@/lib/utils";
import {
	childDelegationsOf,
	delegationInstruction,
	isDelegationActive,
	parentDelegationOf,
	relativeTimeLabel,
} from "./delegation-lineage";
import { useWorkspaceDelegations } from "./use-workspace-delegations";

const STATUS_DOT: Record<Delegation["status"], string> = {
	draft: "bg-muted-foreground/45",
	queued: "bg-sky-500/70",
	running: "bg-sky-500 animate-pulse",
	review_pending: "bg-amber-500",
	completed: "bg-emerald-500",
	failed: "bg-destructive",
	cancelled: "bg-muted-foreground/45",
};

/**
 * Lineage of the open conversation: the child sessions it delegated to (with
 * status, open and cancel) and, for a delegated child, the session that asked.
 * Hidden when the conversation has neither.
 */
export function DelegationLineageMenu({
	workspaceId,
	sessionId,
	sessions,
	onSelectSession,
}: {
	workspaceId: string | null;
	sessionId: string | null;
	sessions: readonly WorkspaceSessionSummary[];
	onSelectSession: (sessionId: string) => void;
}) {
	const { t, i18n } = useTranslation("common");
	const queryClient = useQueryClient();
	const { data: delegations = [] } = useWorkspaceDelegations(workspaceId);
	const [cancellingId, setCancellingId] = useState<string | null>(null);
	const children = useMemo(
		() => childDelegationsOf(delegations, sessionId),
		[delegations, sessionId],
	);
	const parent = useMemo(
		() => parentDelegationOf(delegations, sessionId),
		[delegations, sessionId],
	);
	if (children.length === 0 && !parent) {
		return null;
	}
	const activeCount = children.filter(isDelegationActive).length;
	const locale = i18n.resolvedLanguage ?? i18n.language;
	const titleOf = (id: string | null) =>
		sessions.find((summary) => summary.session.id === id)?.thread.title ?? null;
	const label = t("delegation.lineage.button", { count: children.length });

	const cancel = async (delegation: Delegation) => {
		setCancellingId(delegation.id);
		try {
			await cancelRunningDelegation({ delegationId: delegation.id, reason: null });
			await queryClient.invalidateQueries({ queryKey: ["delegations", workspaceId] });
		} catch (error) {
			toast.error(t("delegation.lineage.cancelFailed"), {
				description: error instanceof Error ? error.message : undefined,
			});
		} finally {
			setCancellingId(null);
		}
	};

	return (
		<DropdownMenu>
			<Tooltip>
				<TooltipTrigger asChild>
					<DropdownMenuTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							aria-label={label}
							className={cn(
								"relative",
								activeCount > 0
									? "text-sky-600 hover:text-sky-500 dark:text-sky-400"
									: "text-muted-foreground hover:text-foreground",
							)}
						>
							<GitFork className="size-3.5" strokeWidth={1.9} />
							{activeCount > 0 ? (
								<span className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full border border-background bg-sky-500 px-1 text-[9px] font-medium leading-none text-white">
									{activeCount}
								</span>
							) : null}
						</Button>
					</DropdownMenuTrigger>
				</TooltipTrigger>
				<TooltipContent side="bottom">{label}</TooltipContent>
			</Tooltip>
			<DropdownMenuContent
				align="end"
				className="max-h-[min(70vh,28rem)] w-80 overflow-y-auto overscroll-contain"
			>
				{parent ? (
					<>
						<DropdownMenuLabel className="text-[11px] text-muted-foreground">
							{t("delegation.lineage.delegatedBy")}
						</DropdownMenuLabel>
						<DropdownMenuItem
							className="gap-2"
							onSelect={() => onSelectSession(parent.parentSessionId)}
						>
							<CornerLeftUp className="size-3.5 shrink-0 text-muted-foreground" />
							<span className="min-w-0 flex-1 truncate">
								{titleOf(parent.parentSessionId) ?? t("delegation.lineage.parentSession")}
							</span>
						</DropdownMenuItem>
					</>
				) : null}
				{parent && children.length > 0 ? <DropdownMenuSeparator /> : null}
				{children.length > 0 ? (
					<DropdownMenuLabel className="text-[11px] text-muted-foreground">
						{t("delegation.lineage.title")}
					</DropdownMenuLabel>
				) : null}
				{children.map((delegation) => {
					const active = isDelegationActive(delegation);
					const when = relativeTimeLabel(
						delegation.startedAt ?? delegation.createdAt,
						locale,
					);
					return (
						<DropdownMenuItem
							key={delegation.id}
							disabled={!delegation.childSessionId}
							onSelect={() => {
								if (delegation.childSessionId) onSelectSession(delegation.childSessionId);
							}}
							className="items-start gap-2 py-2"
						>
							<span
								aria-hidden
								className={cn("mt-1.5 size-1.5 shrink-0 rounded-full", STATUS_DOT[delegation.status])}
							/>
							<ProviderIcon provider={delegation.targetProviderId} className="mt-0.5 size-3.5 shrink-0" />
							<span className="min-w-0 flex-1">
								<span className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
									<span className="font-medium text-foreground/85">
										{t(`inspector.delegations.mode.${delegation.mode}`, {
											defaultValue: delegation.mode,
										})}
									</span>
									<span aria-hidden>·</span>
									<span>
										{t(`inspector.delegations.status.${delegation.status}`, {
											defaultValue: delegation.status,
										})}
									</span>
									{when ? (
										<>
											<span aria-hidden>·</span>
											<span>{when}</span>
										</>
									) : null}
								</span>
								<span className="mt-0.5 line-clamp-2 text-[12px] leading-4 text-foreground">
									{delegationInstruction(delegation)}
								</span>
							</span>
							{active ? (
								<button
									type="button"
									disabled={cancellingId === delegation.id}
									aria-label={t("delegation.lineage.cancel")}
									title={t("delegation.lineage.cancel")}
									onClick={(event) => {
										event.preventDefault();
										event.stopPropagation();
										void cancel(delegation);
									}}
									className="mt-0.5 grid size-5 shrink-0 place-items-center rounded-sm text-muted-foreground/70 hover:bg-accent hover:text-destructive"
								>
									{cancellingId === delegation.id ? (
										<LoaderCircle className="size-3 animate-spin" />
									) : (
										<Square className="size-3" />
									)}
								</button>
							) : null}
						</DropdownMenuItem>
					);
				})}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
