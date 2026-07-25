import { cva } from "class-variance-authority";
import { Archive, Boxes, GitBranch, Loader2, RotateCcw, Trash2 } from "lucide-react";
import { memo, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
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
	branchToneFromWorkspace,
	initialsFromWorkspaceLabel,
	workspaceRailBranchToneClasses,
	workspaceRailDisplayTitle,
} from "./workspace-rail-shared";

const rowVariants = cva(
	"group/dccRailRow relative min-h-[54px] select-none cursor-pointer rounded-md px-2.5 text-[13px]",
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
	onRestoreWorkspace?: (workspaceId: string) => void;
	onDeleteWorkspace?: (workspaceId: string) => void;
};

const recapToneClass: Record<WorkspaceRecapTone, string> = {
	neutral: "text-muted-foreground/70",
	working: "text-amber-600/85 dark:text-amber-300/80",
	attention: "text-amber-700 dark:text-amber-300",
	ready: "text-emerald-700 dark:text-emerald-300",
	done: "text-violet-700 dark:text-violet-300",
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

	const time = formatCompactElapsedTime(now - timestampMs);
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
			className="flex size-[16px] shrink-0 items-center justify-center rounded-[5px] border border-transparent bg-accent/65 text-[8px] font-semibold uppercase text-foreground ring-1 ring-border/50"
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
		onRestoreWorkspace,
		onDeleteWorkspace,
	}: WorkspaceRailRowProps) {
		const { t } = useTranslation("common");
		const branchTone = branchToneFromWorkspace(workspace);
		const displayTitle = workspaceRailDisplayTitle(workspace);
		const workspacePath = workspace.worktreePath ?? workspace.rootPath ?? null;
		const railRecap = useWorkspaceRailRecap({
			workspacePath,
			branch: workspace.branch,
			activity: activity ?? null,
			enabled: metadataEnabled && workspace.status !== "archived",
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
		const [pendingAction, setPendingAction] = useState<
			"restore" | "delete" | null
		>(null);

		const runRowAction = (
			action: "restore" | "delete",
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

		return (
			<div className="px-[2px]">
				<div
					role="button"
					tabIndex={0}
					aria-current={selected ? "location" : undefined}
					aria-label={`Open workspace ${displayTitle}`}
					data-active={selected ? "true" : "false"}
					data-workspace-id={workspace.id}
					onClick={() => {
						if (isPending) {
							return;
						}
						onSelect?.(workspace.id);
					}}
					onKeyDown={(event) => {
						if (event.key === "Enter" || event.key === " ") {
							event.preventDefault();
							if (isPending) {
								return;
							}
							onSelect?.(workspace.id);
						}
					}}
					className={cn(
						rowVariants({ active: selected }),
						"w-full text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/50",
						workspace.status === "archived" && !selected && "opacity-50",
						isPending && "pointer-events-none opacity-60",
					)}
				>
					<div className="flex h-[30px] min-w-0 items-center gap-2">
						<WorkspaceRailAvatar title={displayTitle} subtitle={workspace.name} />
						<div className="flex min-w-0 flex-1 items-center gap-2">
							<div className="row-content-fade flex min-w-0 flex-1 items-center gap-2">
								{workspace.bundleId ? (
									<Boxes
										className="size-[13px] shrink-0 text-cyan-400"
										strokeWidth={1.9}
										aria-hidden
									/>
								) : (
									<GitBranch
										className={cn(
											"size-[13px] shrink-0",
											workspaceRailBranchToneClasses[branchTone],
										)}
										strokeWidth={1.9}
										aria-hidden
									/>
								)}
								<span
									className={cn(
										"min-w-0 truncate leading-tight",
										selected ? "font-medium text-foreground" : "font-medium",
									)}
								>
									{displayTitle}
								</span>
								{workspace.memberWorkspaceIds &&
								workspace.memberWorkspaceIds.length > 1 ? (
									<span className="shrink-0 rounded-full bg-cyan-500/15 px-1.5 text-[9.5px] font-semibold text-cyan-300">
										{workspace.memberWorkspaceIds.length}
									</span>
								) : null}
							</div>
							{activity && (
								<span className="ml-auto flex shrink-0 items-center gap-1 transition-opacity group-hover/dccRailRow:opacity-0 group-focus-within/dccRailRow:opacity-0">
									<span
										aria-hidden
										className={cn(
											"size-[6px] shrink-0 rounded-full",
											activity.state === "active" &&
												"bg-amber-400 animate-pulse",
											activity.state === "completed" && "bg-emerald-500",
											activity.state === "aborted" && "bg-destructive",
										)}
									/>
									<span
										className={cn(
											"flex items-center gap-1 whitespace-nowrap text-[10px] font-medium leading-none",
											activity.state === "active" && "text-amber-500",
											activity.state === "completed" && "text-emerald-500",
											activity.state === "aborted" && "text-destructive",
										)}
									>
										{t(`sidebar.agentState.${activity.state}`)}
										<span aria-hidden className="opacity-35">
											·
										</span>
										<span className="opacity-65">
											<WorkspaceActivityTime activity={activity} />
										</span>
									</span>
								</span>
							)}
						</div>
						<div
							className={cn(
								"group/actions absolute right-2.5 top-[3px] flex items-center gap-0.5 transition-opacity group-hover/dccRailRow:opacity-100 group-focus-within/dccRailRow:opacity-100",
								isPending ? "opacity-100" : "opacity-0",
							)}
						>
							{workspace.status === "archived" ? (
								<>
									{onRestoreWorkspace && (
										<Button
											type="button"
											variant="ghost"
											size="icon-xs"
											aria-label={
												pendingAction === "restore"
													? t("sidebar.restoringWorkspace")
													: "Restore workspace"
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
									)}
									{onDeleteWorkspace && (
										<Button
											type="button"
											variant="ghost"
											size="icon-xs"
											aria-label={
												pendingAction === "delete"
													? t("sidebar.deletingWorkspace")
													: "Delete workspace permanently"
											}
											disabled={isPending}
											className="text-muted-foreground/60 hover:text-destructive disabled:opacity-100"
											onClick={(event) => {
												event.stopPropagation();
												runRowAction("delete", onDeleteWorkspace);
											}}
										>
											{pendingAction === "delete" ? (
												<Loader2
													className="size-3.5 animate-spin text-destructive"
													strokeWidth={2}
													aria-hidden
												/>
											) : (
												<Trash2
													className="size-3.5"
													strokeWidth={2}
													aria-hidden
												/>
											)}
										</Button>
									)}
								</>
							) : (
								onArchiveWorkspace && (
									<Button
										type="button"
										variant="ghost"
										size="icon-xs"
										aria-label="Archive workspace"
										className="text-muted-foreground/60 hover:text-foreground"
										onClick={(event) => {
											event.stopPropagation();
											onArchiveWorkspace(workspace.id);
										}}
									>
										<Archive
											className="size-3.5"
											strokeWidth={2}
											aria-hidden
										/>
									</Button>
								)
							)}
						</div>
					</div>
					{recapMessage && railRecap ? (
						<p
							className={cn(
								"truncate pb-1 pl-6 text-[10.5px] leading-4",
								recapToneClass[railRecap.recap.tone],
							)}
							title={recapTitle}
						>
							{recapMessage}
						</p>
					) : null}
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
		previous.onRestoreWorkspace === next.onRestoreWorkspace &&
		previous.onDeleteWorkspace === next.onDeleteWorkspace,
);
