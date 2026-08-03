import { cva } from "class-variance-authority";
import {
	CircleCheck,
	CirclePause,
	Folder,
	GitBranch,
	Layers3,
	Loader2,
	MoreHorizontal,
	Pencil,
	Pin,
	PinOff,
	RotateCcw,
	ShieldCheck,
	SquareTerminal,
	Trash2,
} from "lucide-react";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
	workspaceRailStatusTakesRecapSlot,
} from "./workspace-rail-shared";
import { useWorkspaceActiveTerminalCount } from "@/features/terminal/use-active-terminal-count";
import { ProjectIdentityGlyph } from "./project-identity";

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
	projectLabel?: string | null;
	projectIcon?: string | null;
	projectColor?: string | null;
	onSelect?: (workspaceId: string) => void;
	onRenameWorkspace?: (workspaceId: string, name: string) => void | Promise<void>;
	onArchiveWorkspace?: (workspaceId: string) => void;
	onCompleteWorkspace?: (workspaceId: string) => void | Promise<void>;
	onRestoreWorkspace?: (workspaceId: string) => void;
	onDeleteWorkspace?: (workspaceId: string) => void;
	onSetWorkspacePinned?: (workspaceId: string, pinned: boolean) => Promise<void>;
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
	bare = false,
}: {
	activity: WorkspaceAgentActivity;
	bare?: boolean;
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
	return activity.state === "active" || bare ? (
		<span className="tabular-nums">{time}</span>
	) : (
		<span className="tabular-nums">
			{t("sidebar.activityTime.ago", { time })}
		</span>
	);
}

function pathLeaf(path: string | null | undefined): string | null {
	const normalized = path?.trim().replace(/[\\/]+$/gu, "") ?? "";
	if (!normalized) return null;
	return normalized.split(/[\\/]/gu).filter(Boolean).at(-1) ?? normalized;
}

function WorkspaceIdentityCard({
	workspace,
	displayTitle,
	projectLabel,
	projectIcon,
	projectColor,
	currentBranch,
	activity,
}: {
	workspace: WorkspaceSummary;
	displayTitle: string;
	projectLabel?: string | null;
	projectIcon?: string | null;
	projectColor?: string | null;
	currentBranch: string;
	activity?: WorkspaceAgentActivity | null;
}) {
	const { t } = useTranslation("common");
	const repositoryName = pathLeaf(workspace.rootPath);
	const projectName = projectLabel?.trim() || repositoryName;
	const showRepositoryName =
		repositoryName && repositoryName.toLocaleLowerCase() !== projectName?.toLocaleLowerCase();
	const memberProjects = [...new Set(workspace.memberProjectNames ?? [])];
	const identityActivity =
		activity ??
		(workspace.updatedAt || workspace.createdAt
			? {
					state: "completed" as const,
					startedAt: null,
					completedAt: workspace.updatedAt ?? workspace.createdAt ?? null,
				}
			: null);

	return (
		<div className="w-[292px] space-y-2.5 p-0.5">
			<div className="flex min-w-0 items-center justify-between gap-4">
				<strong className="truncate text-[12px] font-semibold text-popover-foreground">
					{displayTitle}
				</strong>
				{identityActivity ? (
					<span className="shrink-0 text-[10px] font-medium text-muted-foreground">
						<WorkspaceActivityTime activity={identityActivity} bare />
					</span>
				) : null}
			</div>

			<div className="space-y-1.5 border-t border-border/70 pt-2 text-[11px]">
				{projectName ? (
					<div className="flex min-w-0 items-center gap-2">
						<ProjectIdentityGlyph
							icon={projectIcon}
							color={projectColor}
							size="sm"
							title={projectName}
						/>
						<span className="truncate font-medium">{projectName}</span>
					</div>
				) : null}
				{showRepositoryName ? (
					<div className="flex min-w-0 items-center gap-2 text-muted-foreground">
						<Folder className="size-3.5 shrink-0" strokeWidth={1.8} aria-hidden />
						<span className="truncate">{repositoryName}</span>
					</div>
				) : null}
				<div className="flex min-w-0 items-center gap-2 text-muted-foreground">
					<GitBranch className="size-3.5 shrink-0" strokeWidth={1.8} aria-hidden />
					<span className="min-w-0 truncate text-popover-foreground/90">
						{currentBranch}
					</span>
					{workspace.baseBranch && workspace.baseBranch !== currentBranch ? (
						<span className="ml-auto shrink-0 text-[9.5px]">
							{t("sidebar.workspaceIdentity.base", {
								branch: workspace.baseBranch,
							})}
						</span>
					) : null}
				</div>
				<div className="flex min-w-0 items-center gap-2 text-emerald-600 dark:text-emerald-400">
					<ShieldCheck className="size-3.5 shrink-0" strokeWidth={1.8} aria-hidden />
					<span>{t("sidebar.workspaceIdentity.protectedWorktree")}</span>
				</div>
				{memberProjects.length > 1 ? (
					<div className="flex min-w-0 items-start gap-2 border-t border-border/60 pt-1.5 text-muted-foreground">
						<Layers3 className="mt-px size-3.5 shrink-0" strokeWidth={1.8} aria-hidden />
						<span className="line-clamp-2">
							{t("sidebar.workspaceIdentity.coordinatedProjects", {
								count: memberProjects.length,
								projects: memberProjects.join(" · "),
							})}
						</span>
					</div>
				) : null}
			</div>
		</div>
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
		projectLabel,
		projectIcon,
		projectColor,
		onSelect,
		onRenameWorkspace,
		onArchiveWorkspace,
		onCompleteWorkspace,
		onRestoreWorkspace,
		onDeleteWorkspace,
		onSetWorkspacePinned,
	}: WorkspaceRailRowProps) {
		const { t } = useTranslation("common");
		const activeTerminalCount = useWorkspaceActiveTerminalCount([
			workspace.id,
			...(workspace.memberWorkspaceIds ?? []),
		]);
		const displayTitle = workspaceRailDisplayTitle(workspace);
		const workspacePath = workspace.worktreePath ?? workspace.rootPath ?? null;
		const railState = useWorkspaceRailRecap({
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
		const railRecap = railState.recap;
		const recapMessage = railRecap
			? t(
					`inspector.recap.messages.${railRecap.recap.messageKey}`,
					railRecap.recap.params,
				)
			: null;
		const hasPriorityWorkspaceStatus = workspaceRailStatusTakesRecapSlot(
			workspace.status,
			Boolean(recapMessage),
		);
		const workspaceStatusMessage =
			workspace.status === "initializing"
				? t("sidebar.workspaceState.initializing")
				: workspace.status === "setup_pending" &&
						hasPriorityWorkspaceStatus
					? t("sidebar.workspaceState.setupPending")
					: workspace.status === "ready" && !activity && !recapMessage
						? t("sidebar.workspaceState.readyToStart")
						: null;
		const [pendingAction, setPendingAction] = useState<
			"restore" | "complete" | "archive" | null
		>(null);
		const [isEditing, setIsEditing] = useState(false);
		const [draftName, setDraftName] = useState(displayTitle);
		const [isRenaming, setIsRenaming] = useState(false);
		const [identityOpen, setIdentityOpen] = useState(false);
		const renameInputRef = useRef<HTMLInputElement>(null);
		const renameSubmissionRef = useRef(false);

		useEffect(() => {
			if (!isEditing) {
				setDraftName(displayTitle);
				return;
			}
			renameInputRef.current?.focus();
			renameInputRef.current?.select();
		}, [displayTitle, isEditing]);

		const cancelRename = () => {
			if (isRenaming) return;
			setDraftName(displayTitle);
			setIsEditing(false);
		};
		const submitRename = async () => {
			if (renameSubmissionRef.current || !onRenameWorkspace) return;
			const name = draftName.replace(/\s+/gu, " ").trim();
			if (!name) {
				toast.error(t("sidebar.renameWorkspaceEmpty"));
				renameInputRef.current?.focus();
				return;
			}
			if (name === displayTitle) {
				setIsEditing(false);
				return;
			}
			renameSubmissionRef.current = true;
			setIsRenaming(true);
			try {
				await onRenameWorkspace(workspace.id, name);
				setIsEditing(false);
			} catch (error) {
				toast.error(
					error instanceof Error ? error.message : t("sidebar.renameWorkspaceError"),
				);
				renameInputRef.current?.focus();
			} finally {
				renameSubmissionRef.current = false;
				setIsRenaming(false);
			}
		};

		const runRowAction = (
			action: "restore" | "complete" | "archive",
			handler: (workspaceId: string) => void | Promise<void>,
		) => {
			if (pendingAction) {
				return;
			}
			setPendingAction(action);
			Promise.resolve(handler(workspace.id)).finally(() => {
				setPendingAction(null);
			});
		};

		const isPending = pendingAction !== null;
		const canSelect =
			workspace.status !== "archived" && workspace.status !== "completed";
		const canPin = canSelect && Boolean(onSetWorkspacePinned);
		const hasWorkspaceMenu = Boolean(onRenameWorkspace) || canPin;

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
						<Tooltip
							open={!isEditing && identityOpen}
							onOpenChange={setIdentityOpen}
							delayDuration={450}
						>
							<TooltipTrigger asChild>
								<div className="min-w-0 flex-1">
							<div
								className={cn(
									"flex min-w-0 items-center gap-1.5 transition-[padding]",
									onRenameWorkspace &&
										workspace.status === "ready" &&
										onCompleteWorkspace &&
										onArchiveWorkspace
										? "group-hover/dccRailRow:pr-[4.5rem] group-focus-within/dccRailRow:pr-[4.5rem]"
										: onRenameWorkspace
											? "group-hover/dccRailRow:pr-12 group-focus-within/dccRailRow:pr-12"
											: workspace.status === "ready" &&
													onCompleteWorkspace &&
													onArchiveWorkspace
												? "group-hover/dccRailRow:pr-12 group-focus-within/dccRailRow:pr-12"
												: "group-hover/dccRailRow:pr-6 group-focus-within/dccRailRow:pr-6",
								)}
							>
								{isEditing ? (
									<input
										ref={renameInputRef}
										value={draftName}
										disabled={isRenaming}
										maxLength={120}
										aria-label={t("sidebar.renameWorkspaceInput")}
										className="h-6 min-w-0 flex-1 rounded border border-ring/60 bg-background px-1.5 text-[12px] font-medium text-foreground outline-none ring-2 ring-ring/15 disabled:opacity-60"
										onClick={(event) => event.stopPropagation()}
										onChange={(event) => setDraftName(event.target.value)}
										onBlur={() => void submitRename()}
										onKeyDown={(event) => {
											event.stopPropagation();
											if (event.key === "Enter") {
												event.preventDefault();
												void submitRename();
											} else if (event.key === "Escape") {
												event.preventDefault();
												cancelRename();
											}
										}}
									/>
								) : (
									<span
										className="min-w-0 truncate font-medium leading-5 text-foreground"
									>
										{displayTitle}
									</span>
								)}
								{workspace.memberWorkspaceIds &&
								workspace.memberWorkspaceIds.length > 1 ? (
									<span className="shrink-0 rounded-full bg-foreground/[0.08] px-1.5 text-[9.5px] font-semibold tabular-nums text-muted-foreground">
										{workspace.memberWorkspaceIds.length}
									</span>
								) : null}
								{workspace.pinnedAt ? (
									<Pin
										className="size-3 shrink-0 rotate-[-12deg] text-muted-foreground/65"
										strokeWidth={1.9}
										aria-label={t("sidebar.pinnedWorkspace")}
									/>
								) : null}
								{activeTerminalCount > 0 ? (
									<span
										className="ml-auto inline-flex h-4 shrink-0 items-center gap-1 rounded-full bg-sky-500/12 px-1.5 text-[9.5px] font-semibold tabular-nums text-sky-700 dark:text-sky-300"
										title={t("sidebar.activeTerminals", { count: activeTerminalCount })}
									>
										<SquareTerminal className="size-2.5" aria-hidden />
										{activeTerminalCount}
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
								>
									{recapMessage}
								</p>
							) : null}
								</div>
							</TooltipTrigger>
							<TooltipContent
								side="right"
								align="start"
								sideOffset={10}
								className="!block !w-auto !max-w-none !items-stretch !rounded-xl !bg-popover !p-3 !text-popover-foreground shadow-xl ring-1 ring-border/80"
							>
								<WorkspaceIdentityCard
									workspace={workspace}
									displayTitle={displayTitle}
									projectLabel={projectLabel}
									projectIcon={projectIcon}
									projectColor={projectColor}
									currentBranch={railState.currentBranch}
									activity={activity}
								/>
							</TooltipContent>
						</Tooltip>
						<div
							className={cn(
								"group/actions absolute right-2 top-1.5 flex items-center gap-0.5 rounded-md bg-sidebar/90 pl-1 transition-opacity group-hover/dccRailRow:opacity-100 group-focus-within/dccRailRow:opacity-100",
								isPending ? "opacity-100" : "opacity-0",
							)}
						>
							{hasWorkspaceMenu ? (
								<DropdownMenu>
									<DropdownMenuTrigger asChild>
										<Button
											type="button"
											variant="ghost"
											size="icon-xs"
											aria-label={t("sidebar.workspaceActions")}
											disabled={isPending || isRenaming}
											className="text-muted-foreground/60 hover:text-foreground"
											onClick={(event) => event.stopPropagation()}
										>
											<MoreHorizontal className="size-3.5" strokeWidth={2} aria-hidden />
										</Button>
									</DropdownMenuTrigger>
									<DropdownMenuContent align="end" sideOffset={5}>
										{canPin ? (
											<DropdownMenuItem
												className="gap-2 text-[13px]"
												onSelect={(event) => {
													event.preventDefault();
													void onSetWorkspacePinned?.(
														workspace.id,
														!workspace.pinnedAt,
													).catch((error) =>
														toast.error(
															error instanceof Error
																? error.message
																: t("sidebar.pinWorkspaceError"),
														),
													);
												}}
											>
												{workspace.pinnedAt ? (
													<PinOff className="size-3.5" strokeWidth={1.9} aria-hidden />
												) : (
													<Pin className="size-3.5" strokeWidth={1.9} aria-hidden />
												)}
												{workspace.pinnedAt
													? t("sidebar.unpinWorkspace")
													: t("sidebar.pinWorkspace")}
											</DropdownMenuItem>
										) : null}
										{onRenameWorkspace ? (
											<DropdownMenuItem
												className="gap-2 text-[13px]"
												onSelect={(event) => {
													event.preventDefault();
													setIdentityOpen(false);
													setDraftName(displayTitle);
													setIsEditing(true);
												}}
											>
												<Pencil className="size-3.5" strokeWidth={2} aria-hidden />
												{t("sidebar.renameWorkspace")}
											</DropdownMenuItem>
										) : null}
									</DropdownMenuContent>
								</DropdownMenu>
							) : null}
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
													runRowAction("archive", onArchiveWorkspace);
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
		previous.projectLabel === next.projectLabel &&
		previous.projectIcon === next.projectIcon &&
		previous.projectColor === next.projectColor &&
		previous.workspace === next.workspace &&
		previous.onSelect === next.onSelect &&
		previous.onRenameWorkspace === next.onRenameWorkspace &&
		previous.onArchiveWorkspace === next.onArchiveWorkspace &&
		previous.onCompleteWorkspace === next.onCompleteWorkspace &&
		previous.onRestoreWorkspace === next.onRestoreWorkspace &&
		previous.onDeleteWorkspace === next.onDeleteWorkspace &&
		previous.onSetWorkspacePinned === next.onSetWorkspacePinned,
);
