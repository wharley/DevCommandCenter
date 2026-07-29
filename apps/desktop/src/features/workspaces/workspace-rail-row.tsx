import { cva } from "class-variance-authority";
import { CircleCheck, CirclePause, Loader2, RotateCcw, Trash2 } from "lucide-react";
import { memo, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { WorkspaceRecapTone } from "@/features/inspector/workspace-recap";
import type { WorkspaceAgentActivity } from "./use-workspace-agent-states";
import { useWorkspaceRailRecap } from "./use-workspace-rail-recap";
import {
	formatCompactElapsedTime,
	workspaceActivityTimestamp,
} from "./workspace-rail-time";
import type { WorkspaceSummary } from "./types";
import {
	initialsFromWorkspaceLabel,
	workspaceRailDisplayTitle,
} from "./workspace-rail-shared";

const rowVariants = cva(
	"group/dccRailRow relative min-h-[70px] select-none cursor-pointer rounded-md px-2.5 py-1.5 text-[13px] transition-colors",
	{
		variants: {
			active: {
				true: "workspace-row-selected text-foreground",
				false: "text-foreground/80 hover:bg-accent/60",
			},
		},
		defaultVariants: {
			active: false,
		},
	},
);

export type WorkspaceRailRowProps = {
	workspace: WorkspaceSummary;
	selected: boolean;
	activity?: WorkspaceAgentActivity | null;
	metadataEnabled?: boolean;
	onSelect?: (workspaceId: string) => void;
	onArchiveWorkspace?: (workspaceId: string) => void;
	onCompleteWorkspace?: (workspaceId: string) => void | Promise<void>;
	onRestoreWorkspace?: (workspaceId: string) => void;
	onDeleteWorkspace?: (workspaceId: string) => void;
};

/**
 * The recap line sits under the workspace name, so it is graded by value, not by
 * hue: only `attention` (something is blocking the branch) spends chroma, the
 * rest step down in neutral weight. Five tinted lines in a list read as a
 * rainbow and drown out the names above them.
 */
const recapToneClass: Record<WorkspaceRecapTone, string> = {
	neutral: "text-muted-foreground/80",
	working: "text-muted-foreground",
	attention: "text-amber-700 dark:text-amber-300/90",
	ready: "text-foreground/70",
	done: "text-muted-foreground/80",
};

function WorkspaceActivityTime({
	activity,
}: {
	activity: WorkspaceAgentActivity;
}) {
	const { t } = useTranslation("common");
	const timestamp = workspaceActivityTimestamp(activity);
	const timestampMs = timestamp ? Date.parse(timestamp) : Number.NaN;
	const [now, setNow] = useState(() => Date.now());
	const unitLabels = useMemo(
		() => ({
			second: t("sidebar.activityTime.units.second"),
			minute: t("sidebar.activityTime.units.minute"),
			hour: t("sidebar.activityTime.units.hour"),
			day: t("sidebar.activityTime.units.day"),
			month: t("sidebar.activityTime.units.month"),
			months: t("sidebar.activityTime.units.months"),
		}),
		[t],
	);

	useEffect(() => {
		if (Number.isNaN(timestampMs)) {
			return;
		}
		setNow(Date.now());
		const intervalMs = activity.state === "active" ? 1_000 : 60_000;
		const interval = window.setInterval(() => setNow(Date.now()), intervalMs);
		return () => window.clearInterval(interval);
	}, [activity.state, timestampMs]);

	if (Number.isNaN(timestampMs)) {
		return null;
	}

	const time = formatCompactElapsedTime(now - timestampMs, unitLabels);
	return activity.state === "active" ? (
		<span className="tabular-nums">{time}</span>
	) : (
		<span className="tabular-nums">
			{t("sidebar.activityTime.ago", { time })}
		</span>
	);
}

function WorkspaceRailAvatar({
	title,
	subtitle,
}: {
	title: string;
	subtitle: string;
}) {
	const initials = initialsFromWorkspaceLabel(subtitle || title);

	return (
		<span
			aria-hidden
			className="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-md border border-transparent bg-accent/70 text-[8.5px] font-semibold uppercase text-foreground ring-1 ring-border/60"
		>
			{initials}
		</span>
	);
}

export const WorkspaceRailRowItem = memo(
	function WorkspaceRailRowItem({
		workspace,
		selected,
		activity,
		metadataEnabled = true,
		onSelect,
		onArchiveWorkspace,
		onCompleteWorkspace,
		onRestoreWorkspace,
		onDeleteWorkspace,
	}: WorkspaceRailRowProps) {
		const { t } = useTranslation("common");
		const displayTitle = workspaceRailDisplayTitle(workspace);
		const workspacePath = workspace.worktreePath ?? workspace.rootPath ?? null;
		const railRecap = useWorkspaceRailRecap({
			workspacePath,
			branch: workspace.branch,
			activity: activity ?? null,
			enabled:
				metadataEnabled &&
				workspace.status !== "archived" &&
				workspace.status !== "completed",
			onPullRequestMerged:
				workspace.status === "ready" &&
				!workspace.bundleId &&
				onCompleteWorkspace
					? () => onCompleteWorkspace(workspace.id)
					: undefined,
		});
		const recapMessage = railRecap
			? t(
					`inspector.recap.messages.${railRecap.recap.messageKey}`,
					railRecap.recap.params,
				)
			: null;
		const recapTitle =
			recapMessage && railRecap?.prTitle
				? `${recapMessage} — ${railRecap.prTitle}`
				: recapMessage ?? undefined;
		const hasPriorityWorkspaceStatus =
			workspace.status === "initializing" ||
			workspace.status === "setup_pending";
		const workspaceStatusMessage =
			workspace.status === "initializing"
				? t("sidebar.workspaceState.initializing")
				: workspace.status === "setup_pending"
					? t("sidebar.workspaceState.setupPending")
					: workspace.status === "ready" && !activity && !recapMessage
						? t("sidebar.workspaceState.readyToStart")
						: null;
		const [pendingAction, setPendingAction] = useState<
			"restore" | "complete" | null
		>(null);

		const runRowAction = (
			action: "restore" | "complete",
			handler: (workspaceId: string) => void | Promise<void>,
		) => {
			if (pendingAction) {
				return;
			}
			setPendingAction(action);
			Promise.resolve(handler(workspace.id)).catch(() => {
				// On failure the row stays; clear pending so it can be retried.
				setPendingAction(null);
			});
		};

		const isPending = pendingAction !== null;
		const canSelect =
			workspace.status !== "archived" && workspace.status !== "completed";

		return (
			<div className="px-[2px]">
				<div
					role={canSelect ? "button" : undefined}
					tabIndex={canSelect ? 0 : -1}
					aria-current={selected ? "location" : undefined}
					aria-label={
						canSelect
							? t("sidebar.openWorkspace", { label: displayTitle })
							: undefined
					}
					data-active={selected ? "true" : "false"}
					data-workspace-id={workspace.id}
					onClick={() => {
						if (isPending) {
							return;
						}
						if (canSelect) {
							onSelect?.(workspace.id);
						}
					}}
					onKeyDown={(event) => {
						if (event.key === "Enter" || event.key === " ") {
							event.preventDefault();
							if (isPending) {
								return;
							}
							if (canSelect) {
								onSelect?.(workspace.id);
							}
						}
					}}
					className={cn(
						rowVariants({ active: selected }),
						"w-full text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/50",
						(workspace.status === "archived" || workspace.status === "completed") &&
							!selected &&
							"opacity-50",
						isPending && "pointer-events-none opacity-60",
					)}
				>
					{selected ? (
						<span
							aria-hidden
							className="absolute inset-y-2 left-0 w-0.5 rounded-full bg-sidebar-primary"
						/>
					) : null}
					<div className="flex min-w-0 items-start gap-2">
						<WorkspaceRailAvatar title={displayTitle} subtitle={workspace.name} />
						<div className="min-w-0 flex-1">
							<div
								className={cn(
									"flex min-w-0 items-center gap-1.5 transition-[padding]",
									workspace.status === "ready" &&
										onCompleteWorkspace &&
										onArchiveWorkspace
										? "group-hover/dccRailRow:pr-12 group-focus-within/dccRailRow:pr-12"
										: "group-hover/dccRailRow:pr-6 group-focus-within/dccRailRow:pr-6",
								)}
							>
								<span
									className="min-w-0 truncate font-medium leading-5 text-foreground"
									title={displayTitle}
								>
									{displayTitle}
								</span>
								{workspace.memberWorkspaceIds &&
								workspace.memberWorkspaceIds.length > 1 ? (
									<span className="shrink-0 rounded-full bg-foreground/[0.08] px-1.5 text-[9.5px] font-semibold tabular-nums text-muted-foreground">
										{workspace.memberWorkspaceIds.length}
									</span>
								) : null}
							</div>
							{workspaceStatusMessage ? (
								<div className="mt-px flex min-w-0 items-center gap-1.5">
									<span
										aria-hidden
										className={cn(
											"size-[6px] shrink-0 rounded-full",
											workspace.status === "initializing" &&
												"animate-pulse bg-muted-foreground/45",
											workspace.status === "setup_pending" &&
												"bg-amber-500/80",
											workspace.status === "ready" &&
												"bg-muted-foreground/35",
										)}
									/>
									<span
										className={cn(
											"truncate text-[10.5px] font-medium leading-4 text-muted-foreground",
											workspace.status === "setup_pending" &&
												"text-amber-700 dark:text-amber-300/90",
										)}
									>
										{workspaceStatusMessage}
									</span>
								</div>
							) : null}
							{activity && !hasPriorityWorkspaceStatus && (
								<div className="mt-px flex min-w-0 items-center gap-1.5">
									<span
										aria-hidden
										className={cn(
											"size-[6px] shrink-0 rounded-full",
											activity.state === "active" &&
												"bg-amber-400 animate-pulse",
											activity.state === "completed" && "bg-emerald-500/80",
											activity.state === "aborted" && "bg-destructive",
										)}
									/>
									<span className="flex min-w-0 items-center gap-1 whitespace-nowrap text-[10.5px] font-medium leading-4 text-muted-foreground">
										<span
											className={cn(
												activity.state === "active" &&
													"text-amber-700 dark:text-amber-300/90",
												activity.state === "completed" &&
													"text-emerald-600 dark:text-emerald-400/90",
												activity.state === "aborted" && "text-destructive/85",
											)}
										>
											{t(`sidebar.agentState.${activity.state}`)}
										</span>
										<span aria-hidden className="opacity-40">
											·
										</span>
										<WorkspaceActivityTime activity={activity} />
									</span>
								</div>
							)}
							{recapMessage && railRecap && !hasPriorityWorkspaceStatus ? (
								<p
									className={cn(
										"truncate text-[10.5px] leading-4",
										recapToneClass[railRecap.recap.tone],
									)}
									title={recapTitle}
								>
									{recapMessage}
								</p>
							) : null}
						</div>
						<div
							className={cn(
								"group/actions absolute right-2 top-1.5 flex items-center gap-0.5 rounded-md bg-sidebar/90 pl-1 transition-opacity group-hover/dccRailRow:opacity-100 group-focus-within/dccRailRow:opacity-100",
								isPending ? "opacity-100" : "opacity-0",
							)}
						>
							{workspace.status === "archived" ? (
								onRestoreWorkspace && (
									<Button
										type="button"
										variant="ghost"
										size="icon-xs"
										aria-label={
											pendingAction === "restore"
												? t("sidebar.restoringWorkspace")
												: t("sidebar.restoreWorkspace")
										}
										disabled={isPending}
										className="text-muted-foreground/60 hover:text-foreground disabled:opacity-100"
										onClick={(event) => {
											event.stopPropagation();
											runRowAction("restore", onRestoreWorkspace);
										}}
									>
										{pendingAction === "restore" ? (
											<Loader2
												className="size-3.5 animate-spin"
												strokeWidth={2}
												aria-hidden
											/>
										) : (
											<RotateCcw
												className="size-3.5"
												strokeWidth={2}
												aria-hidden
											/>
										)}
									</Button>
								)
							) : workspace.status === "completed" ? (
								onDeleteWorkspace && (
									<Button
										type="button"
										variant="ghost"
										size="icon-xs"
										aria-label={t("sidebar.deleteWorkspace")}
										className="text-muted-foreground/60 hover:text-destructive"
										onClick={(event) => {
											event.stopPropagation();
											onDeleteWorkspace(workspace.id);
										}}
									>
										<Trash2
											className="size-3.5"
											strokeWidth={2}
											aria-hidden
										/>
									</Button>
								)
							) : (
								<>
									{workspace.status === "ready" && onCompleteWorkspace ? (
										<Tooltip>
											<TooltipTrigger asChild>
												<Button
													type="button"
													variant="ghost"
													size="icon-xs"
													aria-label={
														pendingAction === "complete"
															? t("sidebar.completingWorkspace")
															: t("sidebar.completeWorkspace")
													}
													disabled={isPending}
													className="text-muted-foreground/60 hover:text-emerald-600 disabled:opacity-100 dark:hover:text-emerald-400"
													onClick={(event) => {
														event.stopPropagation();
														runRowAction("complete", onCompleteWorkspace);
													}}
												>
													{pendingAction === "complete" ? (
														<Loader2
															className="size-3.5 animate-spin"
															strokeWidth={2}
															aria-hidden
														/>
													) : (
														<CircleCheck
															className="size-3.5"
															strokeWidth={2}
															aria-hidden
														/>
													)}
												</Button>
											</TooltipTrigger>
											<TooltipContent side="top">
												{t("sidebar.completeWorkspace")}
											</TooltipContent>
										</Tooltip>
									) : null}
									{onArchiveWorkspace ? (
										<Tooltip>
											<TooltipTrigger asChild>
												<Button
													type="button"
													variant="ghost"
													size="icon-xs"
													aria-label={t("sidebar.moveWorkspaceToWaiting")}
													disabled={isPending}
													className="text-muted-foreground/60 hover:text-amber-600 dark:hover:text-amber-400"
													onClick={(event) => {
														event.stopPropagation();
														onArchiveWorkspace(workspace.id);
													}}
												>
													<CirclePause
														className="size-3.5"
														strokeWidth={2}
														aria-hidden
													/>
												</Button>
											</TooltipTrigger>
											<TooltipContent side="top">
												{t("sidebar.moveWorkspaceToWaiting")}
											</TooltipContent>
										</Tooltip>
									) : null}
								</>
							)}
						</div>
					</div>
				</div>
			</div>
		);
	},
	(previous, next) =>
		previous.selected === next.selected &&
		previous.activity?.state === next.activity?.state &&
		previous.activity?.startedAt === next.activity?.startedAt &&
		previous.activity?.completedAt === next.activity?.completedAt &&
		previous.metadataEnabled === next.metadataEnabled &&
		previous.workspace === next.workspace &&
		previous.onSelect === next.onSelect &&
		previous.onArchiveWorkspace === next.onArchiveWorkspace &&
		previous.onCompleteWorkspace === next.onCompleteWorkspace &&
		previous.onRestoreWorkspace === next.onRestoreWorkspace &&
		previous.onDeleteWorkspace === next.onDeleteWorkspace,
);
