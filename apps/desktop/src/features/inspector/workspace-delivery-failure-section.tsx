import type { WorkspaceDeliveryRecoveryAction } from "@dcc/contracts";
import { useQueryClient } from "@tanstack/react-query";
import {
	AlertTriangle,
	Bot,
	ChevronRight,
	ExternalLink,
	FileWarning,
	GitBranch,
	GitCommitHorizontal,
	Loader2,
	RefreshCw,
	RotateCcw,
	Server,
	TerminalSquare,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { openExternal } from "@/lib/shell-api";
import { cn } from "@/lib/utils";
import { workspaceDeliveryRecoveryExecute } from "@/lib/workspace-api";
import { buildDeliveryFailureComposerPrompt } from "./delivery-failure-format";
import {
	useWorkspaceDeliveryFailure,
	WORKSPACE_DELIVERY_FAILURE_QUERY_KEY,
} from "./use-workspace-delivery-failure";
import { WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY } from "./use-workspace-git-branch-diff";
import { WORKSPACE_GIT_STATUS_QUERY_KEY } from "./use-workspace-git-status";
import { WORKSPACE_PIPELINE_QUERY_KEY } from "./use-workspace-pipeline";
import { WORKSPACE_PR_STATUS_QUERY_KEY } from "./use-workspace-pr-status";
import { WORKSPACE_REVIEW_STATE_QUERY_KEY } from "./use-workspace-review-state";

export function WorkspaceDeliveryFailureSection({
	workspaceRoot,
	branch,
	forgeLogin,
	enabled,
	onPrefillComposer,
}: {
	workspaceRoot: string | null;
	branch: string | null;
	forgeLogin: string | null;
	enabled: boolean;
	onPrefillComposer?: (text: string) => void;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [open, setOpen] = useState(false);
	const [pendingAction, setPendingAction] =
		useState<WorkspaceDeliveryRecoveryAction | null>(null);
	const query = useWorkspaceDeliveryFailure(workspaceRoot, branch, enabled);
	const failure = query.data?.snapshot ?? null;

	if (!enabled || !failure) return null;

	const pushTarget = failure.pushTarget
		? `${failure.pushTarget.remote}/${failure.pushTarget.branch}`
		: null;
	const shortSha = failure.headSha?.slice(0, 8) ?? null;
	const capturedDate = new Date(failure.createdAt);
	const capturedAt = Number.isNaN(capturedDate.getTime())
		? failure.createdAt
		: capturedDate.toLocaleString();
	const canSendToAgent =
		Boolean(onPrefillComposer) &&
		failure.availableActions.includes("send-to-agent");

	const refreshWorkspaceQueries = async () => {
		const root = failure.workspaceRoot;
		await Promise.allSettled([
			queryClient.invalidateQueries({
				queryKey: [WORKSPACE_DELIVERY_FAILURE_QUERY_KEY, root],
			}),
			queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
			}),
			queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, root],
			}),
			queryClient.invalidateQueries({
				queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root],
			}),
			queryClient.invalidateQueries({
				queryKey: [WORKSPACE_REVIEW_STATE_QUERY_KEY, root],
			}),
		]);
	};

	const executeRecovery = async (action: WorkspaceDeliveryRecoveryAction) => {
		const repeatsUpdate =
			action === "retry" &&
			(failure.operation === "fetch" || failure.operation === "pull");
		if (
			(action === "retry" || action === "synchronize") &&
			!window.confirm(
				t(
					action === "retry" && !repeatsUpdate
						? "inspector.deliveryFailure.confirmRetry"
						: "inspector.deliveryFailure.confirmSynchronize",
				),
			)
		) {
			return;
		}

		setPendingAction(action);
		try {
			const result = await workspaceDeliveryRecoveryExecute({
				workspaceRoot: failure.workspaceRoot,
				attemptToken: failure.attemptToken,
				action,
				forgeLogin,
			});

			if (action === "send-to-agent") {
				onPrefillComposer?.(
					buildDeliveryFailureComposerPrompt(result.snapshot),
				);
				toast.success(t("inspector.deliveryFailure.sentToAgent"));
			} else if (action === "open-external") {
				if (!result.snapshot.externalUrl) {
					throw new Error(t("inspector.deliveryFailure.externalUnavailable"));
				}
				await openExternal(result.snapshot.externalUrl);
			} else {
				if (result.refreshPipeline) {
					await queryClient.invalidateQueries({
						queryKey: [
							WORKSPACE_PIPELINE_QUERY_KEY,
							result.snapshot.workspaceRoot,
						],
					});
				}
				toast.success(
					t(
						action === "retry"
							? "inspector.deliveryFailure.retrySuccess"
							: "inspector.deliveryFailure.synchronizeSuccess",
					),
				);
			}
		} catch (error) {
			const message =
				error instanceof Error ? error.message : String(error);
			toast.error(t("inspector.deliveryFailure.actionFailed"), {
				description: message,
			});
		} finally {
			setPendingAction(null);
			await refreshWorkspaceQueries();
		}
	};

	return (
		<div
			data-delivery-failure-section
			className="shrink-0 overflow-hidden rounded-md border border-amber-500/30 bg-amber-500/[0.045]"
		>
			<div className="flex min-w-0 items-center gap-2 px-2.5 py-2">
				<Button
					type="button"
					variant="ghost"
					size="xs"
					className="-ml-1 h-6 min-w-0 flex-1 justify-start gap-2 px-1 text-left hover:bg-transparent"
					onClick={() => setOpen((value) => !value)}
					aria-expanded={open}
				>
					<ChevronRight
						className={cn(
							"size-3.5 shrink-0 transition-transform",
							open && "rotate-90",
						)}
					/>
					<AlertTriangle className="size-3.5 shrink-0 text-amber-600 dark:text-amber-400" />
					<span className="truncate text-[11px] font-medium">
						{t("inspector.deliveryFailure.title")}
					</span>
				</Button>
				<Badge
					variant="outline"
					className={cn(
						"h-5 rounded-full px-1.5 text-[9px] font-medium",
						failure.classification === "unknown"
							? "border-border/70 bg-muted/60 text-muted-foreground"
							: "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-300",
					)}
				>
					{t(
						`inspector.deliveryFailure.classification.${failure.classification}`,
					)}
				</Badge>
			</div>

			{open ? (
				<div className="space-y-2 border-t border-amber-500/15 px-2.5 py-2">
					<p className="text-[9.5px] leading-relaxed text-muted-foreground">
						{t(
							`inspector.deliveryFailure.classificationHint.${failure.classification}`,
						)}
					</p>
					<div className="grid grid-cols-2 gap-1.5">
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<GitBranch className="size-3.5 shrink-0" />
							<span className="truncate">
								{failure.branch ??
									t("inspector.deliveryFailure.branchUnavailable")}
							</span>
						</div>
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<GitCommitHorizontal className="size-3.5 shrink-0" />
							<span className="truncate">
								{shortSha ?? t("inspector.deliveryFailure.commitUnavailable")}
							</span>
						</div>
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<Server className="size-3.5 shrink-0" />
							<span className="truncate">
								{failure.remote ??
									t("inspector.deliveryFailure.remoteUnavailable")}
							</span>
						</div>
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<GitBranch className="size-3.5 shrink-0" />
							<span className="truncate">
								{pushTarget ??
									t("inspector.deliveryFailure.pushTargetUnavailable")}
							</span>
						</div>
					</div>
					<p className="text-[9px] text-muted-foreground">
						{t("inspector.deliveryFailure.capturedContext", {
							operation: t(
								`inspector.deliveryFailure.operation.${failure.operation}`,
							),
							time: capturedAt,
						})}
					</p>

					<div>
						<p className="mb-1 flex items-center gap-1 text-[9px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							<TerminalSquare className="size-3" />
							{t("inspector.deliveryFailure.output")}
						</p>
						<pre className="max-h-36 overflow-auto whitespace-pre-wrap break-words rounded-md bg-background/70 p-2 font-mono text-[9.5px] leading-[1.45] text-foreground/85">
							{failure.output}
						</pre>
						{failure.outputTruncated ? (
							<p className="mt-1 text-[9px] text-amber-700 dark:text-amber-300">
								{t("inspector.deliveryFailure.outputTruncated")}
							</p>
						) : null}
					</div>

					<div>
						<p className="mb-1 flex items-center gap-1 text-[9px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							<FileWarning className="size-3" />
							{t("inspector.deliveryFailure.changedFiles", {
								count: failure.changedFiles.length,
							})}
						</p>
						{failure.changedFiles.length > 0 ? (
							<div className="max-h-24 space-y-0.5 overflow-auto rounded-md bg-background/50 px-2 py-1.5">
								{failure.changedFiles.map((path) => (
									<p
										key={path}
										className="truncate font-mono text-[9.5px] text-muted-foreground"
										title={path}
									>
										{path}
									</p>
								))}
							</div>
						) : (
							<p className="text-[9.5px] text-muted-foreground">
								{t("inspector.deliveryFailure.noChangedFiles")}
							</p>
						)}
						{failure.changedFilesTruncated ? (
							<p className="mt-1 text-[9px] text-amber-700 dark:text-amber-300">
								{t("inspector.deliveryFailure.filesTruncated")}
							</p>
						) : null}
					</div>

					{failure.availableActions.length > 0 ? (
						<div className="flex flex-wrap gap-1.5 border-t border-amber-500/15 pt-2">
							{failure.availableActions.includes("retry") ? (
								<Button
									type="button"
									variant="outline"
									size="xs"
									className="h-7 gap-1.5 text-[9.5px]"
									disabled={pendingAction !== null}
									onClick={() => void executeRecovery("retry")}
								>
									{pendingAction === "retry" ? (
										<Loader2 className="size-3 animate-spin" />
									) : (
										<RotateCcw className="size-3" />
									)}
									{t("inspector.deliveryFailure.retry")}
								</Button>
							) : null}
							{failure.availableActions.includes("synchronize") ? (
								<Button
									type="button"
									variant="outline"
									size="xs"
									className="h-7 gap-1.5 text-[9.5px]"
									disabled={pendingAction !== null}
									onClick={() => void executeRecovery("synchronize")}
								>
									{pendingAction === "synchronize" ? (
										<Loader2 className="size-3 animate-spin" />
									) : (
										<RefreshCw className="size-3" />
									)}
									{t("inspector.deliveryFailure.synchronize")}
								</Button>
							) : null}
							{canSendToAgent ? (
								<Button
									type="button"
									variant="outline"
									size="xs"
									className="h-7 gap-1.5 text-[9.5px]"
									disabled={pendingAction !== null}
									onClick={() => void executeRecovery("send-to-agent")}
								>
									{pendingAction === "send-to-agent" ? (
										<Loader2 className="size-3 animate-spin" />
									) : (
										<Bot className="size-3" />
									)}
									{t("inspector.deliveryFailure.sendToAgent")}
								</Button>
							) : null}
							{failure.availableActions.includes("open-external") ? (
								<Button
									type="button"
									variant="ghost"
									size="xs"
									className="h-7 gap-1.5 text-[9.5px]"
									disabled={pendingAction !== null}
									onClick={() => void executeRecovery("open-external")}
								>
									{pendingAction === "open-external" ? (
										<Loader2 className="size-3 animate-spin" />
									) : (
										<ExternalLink className="size-3" />
									)}
									{t("inspector.deliveryFailure.openExternal")}
								</Button>
							) : null}
						</div>
					) : null}
				</div>
			) : null}
		</div>
	);
}
