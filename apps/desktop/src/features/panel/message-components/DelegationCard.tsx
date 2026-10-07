import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, CheckCircle2, ChevronDown, FileCode2, LoaderCircle } from "lucide-react";
import type { Delegation, ProviderCatalog } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { useWorkspaceDelegations } from "@/features/sessions/use-workspace-delegations";
import { delegationStatusClass } from "@/features/sessions/delegation-status";
import {
	canRerunDelegation,
	describeDelegation,
	rerunTargets,
} from "@/features/sessions/delegation-decisions";
import { eligibleDelegationTargets } from "@/features/sessions/delegation-targets";
import type { WorkspaceMessageDelegation } from "@/features/sessions/session-thread-history.logic";
import type { DelegationCheck } from "@/features/sessions/delegation-verification";
import { delegationInstruction } from "@/features/sessions/delegation-lineage";
import { AgentMascot } from "@/features/providers/agent-mascot";
import { AssistantProse } from "./AssistantProse";
import { MessageTimestamp } from "./message-metadata";

/** Reports longer than this open collapsed, showing their first lines. */
const COLLAPSED_REPORT_CHARS = 320;
const MAX_FILE_CHIPS = 6;

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
	onRerunDelegation,
	onSendDelegationResult,
	childTitle,
	verification,
	onReviewDelegationFile,
}: {
	delegation: WorkspaceMessageDelegation;
	fallbackContent: string;
	createdAt?: string;
	workspaceId: string | null;
	providers: ProviderCatalog["providers"];
	onSelectSession: (sessionId: string) => void;
	onReviewChanges?: () => void;
	onReviewDelegation?: (delegationId: string, path?: string) => void;
	/** Replays this delegation's prompt on another agent. */
	onRerunDelegation?: (input: {
		delegationId: string;
		targetProviderId: string;
	}) => Promise<void>;
	/** Puts this delegation's result in the parent's composer. */
	onSendDelegationResult?: (input: {
		delegationId: string;
		failureReason?: string | null;
	}) => void | Promise<void>;
	/** The child conversation's title — the agent's own name for the task. */
	childTitle?: string | null;
	/** Build/test commands the parent agent ran on the result. */
	verification?: readonly DelegationCheck[];
	/** Opens one changed file of the delegation's worktree in the Inspector. */
	onReviewDelegationFile?: (delegationId: string, path: string) => void;
}) {
	const { t } = useTranslation("common");
	const [isRerunning, setIsRerunning] = useState(false);
	const [reportOpen, setReportOpen] = useState(false);
	// Shares the cache entry with the Inspector and the lineage menu.
	const delegationsQuery = useWorkspaceDelegations(workspaceId);
	const record =
		delegationsQuery.data?.find((item) => item.id === delegation.id) ?? null;

	const status: Delegation["status"] = record?.status ?? PHASE_STATUS[delegation.phase];
	const statusLabel = t(`inspector.delegations.status.${status}`, {
		defaultValue: status,
	});
	const decisions = record ? describeDelegation(record, providers) : null;
	// What ran, in one muted line; how (context, permission) only on hover.
	const agentLabel = decisions ? (decisions.modelLabel ?? decisions.providerLabel) : null;
	const modeLabel = decisions
		? t(`inspector.delegations.mode.${decisions.mode}`, { defaultValue: decisions.mode })
		: null;
	const decisionDetails = decisions
		? [
				decisions.providerLabel,
				...(decisions.modelLabel ? [decisions.modelLabel] : []),
				t(`delegation.contextOptions.${decisions.contextPolicy}`, {
					defaultValue: decisions.contextPolicy,
				}),
				decisions.allowFileEdits
					? t("delegation.card.canEditFiles")
					: t("delegation.card.readOnly"),
			].join(" · ")
		: undefined;
	const title =
		childTitle?.trim() && !childTitle.startsWith("Delegated ")
			? childTitle.trim()
			: record
				? (delegationInstruction(record).split("\n").find((line) => line.trim()) ?? "").trim()
				: "";
	const availableRerunTargets =
		record && onRerunDelegation && canRerunDelegation(record)
			? rerunTargets(record, eligibleDelegationTargets(providers))
			: [];
	const handleRerun = async (targetProviderId: string) => {
		if (!record || !onRerunDelegation || isRerunning) {
			return;
		}
		setIsRerunning(true);
		try {
			await onRerunDelegation({ delegationId: record.id, targetProviderId });
		} finally {
			setIsRerunning(false);
		}
	};
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
	// Finished results can be handed to the parent agent through the composer.
	// Agent-requested delegations already deliver their result on their own.
	const canSendResult =
		Boolean(onSendDelegationResult && record) &&
		record?.origin !== "agent" &&
		(status === "completed" || status === "review_pending" || status === "failed");
	const handleReview = () => {
		if (onReviewDelegation) {
			onReviewDelegation(delegation.id);
			return;
		}
		onReviewChanges?.();
	};

	const isRunning = status === "running" || status === "queued";
	const reportIsLong = summary.length > COLLAPSED_REPORT_CHARS;
	const showSummary = Boolean(summary) && !isRunning;
	const failedChecks = (verification ?? []).filter((check) => !check.ok);

	return (
		<div
			data-message-role="system"
			className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-center px-4"
		>
			<div className="w-full max-w-[42rem] rounded-xl border border-border/70 bg-card/70 px-4 py-3">
				<div className="flex items-start gap-2.5">
					<AgentMascot
						provider={record?.targetProviderId ?? null}
						model={record?.targetModelId ?? null}
						state={isRunning ? "working" : "idle"}
						className="mt-0.5"
					/>
					<div className="min-w-0 flex-1">
						<p
							className="truncate text-[13px] font-medium leading-5 text-foreground"
							title={title || undefined}
						>
							{title || t("delegation.card.title")}
						</p>
						<p
							className="truncate text-[11px] leading-4 text-muted-foreground"
							title={decisionDetails}
						>
							{[modeLabel, agentLabel].filter(Boolean).join(" · ") ||
								t("delegation.card.title")}
						</p>
					</div>
					<Badge
						variant="outline"
						className={cn(
							"mt-0.5 h-5 shrink-0 px-1.5 text-[10px] font-medium",
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

				{isRunning ? (
					<p className="mt-2 text-[12px] leading-5 text-muted-foreground">
						{t("delegation.card.working", {
							agent: agentLabel ?? t("delegation.card.agentFallback"),
						})}
					</p>
				) : null}

				{showSummary ? (
					<div className="mt-2">
						<div
							className={cn(
								"text-[12.5px]",
								reportIsLong &&
									!reportOpen &&
									"max-h-28 overflow-hidden [mask-image:linear-gradient(to_bottom,black_55%,transparent)]",
							)}
						>
							<AssistantProse content={summary} />
						</div>
						{reportIsLong ? (
							<button
								type="button"
								aria-expanded={reportOpen}
								onClick={() => setReportOpen((open) => !open)}
								className="mt-1 inline-flex items-center gap-1 rounded-sm text-[11.5px] font-medium text-muted-foreground hover:text-foreground"
							>
								<ChevronDown
									className={cn("size-3.5 transition-transform", reportOpen && "rotate-180")}
									aria-hidden
								/>
								{reportOpen ? t("delegation.card.hideReport") : t("delegation.card.showReport")}
							</button>
						) : null}
					</div>
				) : null}

				{verification && verification.length > 0 ? (
					<div
						className={cn(
							"mt-2.5 flex flex-wrap items-center gap-x-1.5 gap-y-1 rounded-md px-2 py-1.5 text-[11.5px]",
							failedChecks.length > 0
								? "bg-amber-500/10 text-amber-800 dark:text-amber-200"
								: "bg-emerald-500/10 text-emerald-800 dark:text-emerald-200",
						)}
					>
						{failedChecks.length > 0 ? (
							<AlertTriangle className="size-3.5 shrink-0" aria-hidden />
						) : (
							<CheckCircle2 className="size-3.5 shrink-0" aria-hidden />
						)}
						<span className="font-medium">
							{failedChecks.length > 0
								? t("delegation.card.verifiedWithFailures")
								: t("delegation.card.verified")}
						</span>
						{verification.map((check) => (
							<span
								key={check.command}
								className="rounded bg-background/50 px-1.5 font-mono text-[10.5px] leading-5"
							>
								{check.ok ? "✓" : "✗"} {check.command}
							</span>
						))}
					</div>
				) : null}

				{touchedFiles.length > 0 ? (
					<div className="mt-2.5">
						<p className="text-[11px] text-muted-foreground">
							{t("delegation.card.changedFiles", { count: touchedFiles.length })}
						</p>
						<div className="mt-1 flex flex-wrap gap-1">
							{touchedFiles.slice(0, MAX_FILE_CHIPS).map((path) => {
								const name = path.split("/").filter(Boolean).at(-1) ?? path;
								const canOpen = Boolean(onReviewDelegationFile) && status === "review_pending";
								return (
									<button
										key={path}
										type="button"
										title={path}
										disabled={!canOpen}
										onClick={() => onReviewDelegationFile?.(delegation.id, path)}
										className={cn(
											"inline-flex max-w-[14rem] items-center gap-1 rounded-md border border-border/60 bg-muted/30 px-1.5 py-0.5 font-mono text-[10.5px] text-foreground/80",
											canOpen
												? "hover:border-border hover:bg-accent hover:text-accent-foreground"
												: "cursor-default",
										)}
									>
										<FileCode2 className="size-3 shrink-0 text-muted-foreground" aria-hidden />
										<span className="truncate">{name}</span>
									</button>
								);
							})}
							{touchedFiles.length > MAX_FILE_CHIPS ? (
								<span className="px-1 py-0.5 text-[10.5px] text-muted-foreground">
									+{touchedFiles.length - MAX_FILE_CHIPS}
								</span>
							) : null}
						</div>
					</div>
				) : null}

				<div className="mt-3 flex flex-wrap items-center gap-1.5">
					{showReview ? (
						<Button
							type="button"
							variant={status === "review_pending" ? "default" : "ghost"}
							size="sm"
							className="h-7 px-2.5 text-[11.5px]"
							onClick={handleReview}
						>
							{t("delegation.card.reviewChanges")}
						</Button>
					) : null}
					{childSessionId ? (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="h-7 px-2 text-[11.5px] text-muted-foreground"
							onClick={() => onSelectSession(childSessionId)}
						>
							{t("delegation.card.openChild", {
								agent: agentLabel ?? t("delegation.card.agentFallback"),
							})}
						</Button>
					) : null}
					{canSendResult ? (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="h-7 px-2 text-[11.5px]"
							title={t("delegation.card.sendResultHint")}
							onClick={() =>
								void onSendDelegationResult?.({
									delegationId: delegation.id,
									failureReason: delegation.reason ?? null,
								})
							}
						>
							{t("delegation.card.sendResult")}
						</Button>
					) : null}
					{availableRerunTargets.length > 0 ? (
						<DropdownMenu>
							<DropdownMenuTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="sm"
									className="h-7 gap-1 px-2 text-[11.5px]"
									disabled={isRerunning}
								>
									{isRerunning ? (
										<LoaderCircle className="size-3 animate-spin" aria-hidden />
									) : null}
									{t("delegation.card.rerun")}
								</Button>
							</DropdownMenuTrigger>
							<DropdownMenuContent align="start" className="w-60">
								<DropdownMenuLabel>
									{t("delegation.card.rerunHint")}
								</DropdownMenuLabel>
								{availableRerunTargets.map((target) => (
									<DropdownMenuItem
										key={target.id}
										onSelect={() => void handleRerun(target.id)}
									>
										{target.label}
									</DropdownMenuItem>
								))}
							</DropdownMenuContent>
						</DropdownMenu>
					) : null}
					<span className="ml-auto inline-flex items-center text-[11px] leading-none text-muted-foreground/60">
						<MessageTimestamp createdAt={createdAt} />
					</span>
				</div>
			</div>
		</div>
	);
}
