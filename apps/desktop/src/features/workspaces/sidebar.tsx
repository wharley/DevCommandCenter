import { projectIconValue } from "@dcc/mascots";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
	BarChart3,
	Bot,
	BrushCleaning,
	CircleCheckBig,
	CircleQuestionMark,
	ChevronRight,
	Clock3,
	FolderPlus,
	GitBranch,
	GitPullRequest,
	Loader2,
	MoreHorizontal,
	PanelLeft,
	PanelRight,
	Pin,
	PinOff,
	Plus,
	Settings2,
	Sparkles,
	StickyNote,
	Trash2,
} from "lucide-react";
import {
	memo,
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { AgentsSidebarSection } from "@/features/agents/agents-sidebar-section";
import {
	dismissDailyRecapBubble,
	isRecapBubbleVisible,
	RECAP_BLOCKING_REASONS,
	useDailyRecap,
} from "@/features/agents/daily-recap";
import { useRecapBubbleText } from "@/features/agents/recap-news";
import type { AgentView } from "@/features/agents/use-agents";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { CommandPopoverContent } from "../../components/ui/command-popover";
import {
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
	CommandSeparator,
} from "../../components/ui/command";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "../../components/ui/dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "../../components/ui/dropdown-menu";
import { Popover, PopoverTrigger } from "../../components/ui/popover";
import { FeedbackButton } from "@/features/feedback/feedback-button";
import { Tooltip, TooltipContent, TooltipTrigger } from "../../components/ui/tooltip";
import type { Repository, WorkspaceRemoteBranchDeletionTarget } from "@dcc/contracts";
import { AppUpdateButton, type AppUpdateInfo } from "@/features/updater";
import { cn } from "@/lib/utils";
import { workspaceCleanupScan } from "@/lib/workspace-api";
import {
	dismissCleanupAlert,
	isCleanupAlertDue,
	syncCleanupAlert,
	useCleanupAlert,
} from "@/features/agents/cleanup-alert";
import {
	publishReporterCleanup,
	publishReporterRecap,
	type ReporterCleanupOffer,
	type ReporterRecapNews,
} from "@/features/agents/reporter-store";
import type { WorkspaceSummary } from "./types";
import {
	createInitialRailSectionState,
	readStoredRailSectionState,
	writeStoredRailSectionState,
} from "./workspace-rail-open-state";
import {
	freezeRailGroupOrder,
	projectGroupingKey,
	projectWorkspaceRailGroups,
	splitIdleRailGroups,
	workspaceRailGroupSignal,
} from "./workspace-rail-projection";
import {
	COMPLETED_SECTION_ID,
	findSelectedRailSectionId,
	IDLE_SECTION_ID,
	initialsFromWorkspaceLabel,
	PINNED_SECTION_ID,
	ProjectGroupGlyph,
	RECENT_SECTION_ID,
	WAITING_SECTION_ID,
	workspaceRailDisplayTitle,
} from "./workspace-rail-shared";
import {
	WorkspaceActivityTime,
	WorkspaceRailRowItem,
} from "./workspace-rail-row";
import {
	attentionWorkspaceItems,
	runningWorkspaceActivities,
	useWorkspaceAgentActivities,
	useWorkspaceProviderIds,
	type AttentionReason,
} from "./use-workspace-agent-states";
import { blockersByWorkspace, useWorkspaceBlockers } from "./use-workspace-blockers";
import {
	isWorkspaceResultUnread,
	markWorkspaceResultSeen,
	useSeenWorkspaceResults,
} from "./workspace-seen-results";
import {
	completedCleanupRows,
	completedCleanupSummary,
	deleteCompletedTasks,
	isPreselectedForCleanup,
} from "./completed-cleanup";
import { CompletedCleanupDialog } from "./completed-cleanup-dialog";
import { useCompletedCleanupScan } from "./use-completed-cleanup-scan";
import { ProjectEditDialog } from "./project-edit-dialog";
import { ProjectIdentityGlyph } from "./project-identity";
import { ProviderIcon } from "@/features/providers/provider-icons";
import { repositoryDisplayName } from "./repository-display-name";
import {
	formatDiskBytes,
	workspaceDiskUsageIds,
} from "./workspace-disk-usage";

type VirtualItem =
	| {
			kind: "group-header";
			groupId: string;
			label: string;
			sourceKey?: string;
			rowCount: number;
			canCollapse: boolean;
			headerVariant: "project" | "waiting" | "completed";
	  }
	| { kind: "row"; groupId: string; workspace: WorkspaceSummary }
	| {
			kind: "section-label";
			sectionId: typeof PINNED_SECTION_ID | typeof RECENT_SECTION_ID;
			label: string;
			groupIds: string[];
	  }
	| { kind: "idle-toggle"; count: number; isOpen: boolean }
	| { kind: "group-gap"; size: number }
	| { kind: "bottom-padding" };

const HEADER_HEIGHT = 42;
const SECTION_LABEL_HEIGHT = 24;
const IDLE_TOGGLE_HEIGHT = 30;
const ROW_HEIGHT = 76;
const COMPACT_ROW_HEIGHT = 54;
const GROUP_GAP = 10;
const EMPTY_GROUP_GAP = 8;
const EMPTY_AGENTS: AgentView[] = [];
const BOTTOM_PADDING = 8;
const ATTENTION_PREVIEW_LIMIT = 5;

// Amber = blocked on you; red = interrupted; sky = a finished result you have
// not opened yet (the conventional "unread" dot).
const ATTENTION_DOT_CLASS: Record<AttentionReason, string> = {
	permission: "bg-amber-500",
	input: "bg-amber-500",
	conflicts: "bg-amber-500",
	prConflicts: "bg-amber-500",
	checksFailing: "bg-destructive",
	delegatedReview: "bg-amber-500",
	woke: "bg-sky-500",
	setup: "bg-amber-500/80",
	aborted: "bg-destructive",
	completed: "bg-sky-500",
};

const ATTENTION_TEXT_CLASS: Record<AttentionReason, string> = {
	permission: "text-amber-700 dark:text-amber-300/90",
	input: "text-amber-700 dark:text-amber-300/90",
	conflicts: "text-amber-700 dark:text-amber-300/90",
	prConflicts: "text-amber-700 dark:text-amber-300/90",
	checksFailing: "text-destructive/85",
	delegatedReview: "text-amber-700 dark:text-amber-300/90",
	woke: "text-sky-700 dark:text-sky-300/90",
	setup: "text-amber-700 dark:text-amber-300/90",
	aborted: "text-destructive/85",
	completed: "text-sky-700 dark:text-sky-300/90",
};

function isCompactRailSection(groupId: string) {
	return groupId === WAITING_SECTION_ID || groupId === COMPLETED_SECTION_ID;
}

function getGroupGapSize(previousHasRows: boolean, nextHasRows: boolean) {
	return previousHasRows && nextHasRows ? GROUP_GAP : EMPTY_GROUP_GAP;
}

function WorkspaceRepoPicker({
	repositories,
	isDisabled = false,
	onCreateWorkspaceFromRepository,
	onCreateWorkspace,
	onCloneWorkspace,
}: {
	repositories: Repository[];
	isDisabled?: boolean;
	onCreateWorkspaceFromRepository: (repository: Repository) => void;
	onCreateWorkspace: () => void;
	onCloneWorkspace: () => void;
}) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);
	const orderedRepositories = useMemo(
		() =>
			[...repositories].sort(
				(a, b) => Number(Boolean(b.pinnedAt)) - Number(Boolean(a.pinnedAt)),
			),
		[repositories],
	);

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					aria-label={t("sidebar.openRepoPicker")}
					className="text-muted-foreground hover:text-foreground"
					disabled={isDisabled}
				>
					<Plus className="size-4" strokeWidth={2.2} />
				</Button>
			</PopoverTrigger>
			<CommandPopoverContent
				align="end"
				side="bottom"
				sideOffset={8}
				className="w-80"
			>
				<CommandInput placeholder={t("sidebar.searchReposPlaceholder")} />
				<CommandList>
					<CommandEmpty>{t("sidebar.noRepoFound")}</CommandEmpty>
					<CommandGroup heading={t("sidebar.recentRepos")}>
						{orderedRepositories.map((repository) => (
							<CommandItem
								key={repository.id}
								value={`${repositoryDisplayName(repository)} ${repository.name} ${repository.projectId} ${repository.baseBranch} ${repository.rootPath}`}
								onSelect={() => {
									setOpen(false);
									onCreateWorkspaceFromRepository(repository);
								}}
							>
								<ProjectIdentityGlyph
									icon={projectIconValue(repository)}
									color={repository.color}
									seed={repository.rootPath}
									size="sm"
								/>
								<span className="flex min-w-0 flex-1 flex-col">
									<strong className="truncate">
										{repositoryDisplayName(repository)}
									</strong>
									<span className="truncate text-[var(--dcc-text-muted)]">
										{repository.baseBranch}
									</span>
									<span className="truncate text-[10px] text-muted-foreground">
										{repository.rootPath}
									</span>
								</span>
							</CommandItem>
						))}
					</CommandGroup>
					<CommandSeparator />
					<CommandGroup heading={t("sidebar.actions")}>
						<CommandItem
							value="create workspace"
							onSelect={() => {
								setOpen(false);
								onCreateWorkspace();
							}}
							className="items-start gap-2 py-2"
						>
							<span className="mt-0.5 grid size-6 shrink-0 place-items-center rounded-md bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">
								<Plus className="size-3.5" strokeWidth={2} aria-hidden />
							</span>
							<span className="min-w-0">
								<strong className="block text-[12px] font-medium text-foreground">
									{t("sidebar.openRepoPickerAction")}
								</strong>
								<small className="mt-0.5 block text-[10px] leading-4 text-muted-foreground">
									{t("sidebar.multiProjectActionDescription")}
								</small>
							</span>
						</CommandItem>
						<CommandItem
							value="clone from url"
							onSelect={() => {
								setOpen(false);
								onCloneWorkspace();
							}}
						>
							{t("sidebar.cloneFromUrl")}
						</CommandItem>
					</CommandGroup>
				</CommandList>
			</CommandPopoverContent>
		</Popover>
	);
}

type WorkspacesSidebarProps = {
	collapsed: boolean;
	isCreatingWorkspace?: boolean;
	showAgentStates?: boolean;
	showCompletedDiskUsage?: boolean;
	sessionQueryScope?: string;
	onSelectWorkspace: (workspaceId: string) => void;
	onNewTask: () => void;
	newTaskActive?: boolean;
	onCreateWorkspace: () => void;
	onCloneWorkspace: () => void;
	onCreateWorkspaceFromProject?: (input: {
		projectId: string;
		workspaceRoot: string;
		baseBranch: string;
		label: string;
	}) => void;
	onOpenBranchFromProject?: (input: {
		projectId: string;
		workspaceRoot: string;
		baseBranch: string;
		label: string;
	}) => void;
	repositories: Repository[];
	skillCount?: number;
	appUpdate?: AppUpdateInfo;
	isInstallingUpdate?: boolean;
	onInstallUpdate?: () => void;
	onOpenSettings: () => void;
	onOpenSkills: () => void;
	onOpenUsage: () => void;
	onOpenNotes?: () => void;
	onOpenFeedback?: () => void;
	notesCount?: number;
	onOpenHelp: () => void;
	onOpenPullRequests: () => void;
	pullRequestsActive?: boolean;
	agents?: AgentView[];
	activeAgentId?: string | null;
	onOpenAgent?: (agentId: string) => void;
	onToggleCollapsed: () => void;
	onArchiveWorkspace?: (workspaceId: string) => void;
	/** Snoozes a task until an ISO instant, or wakes it with `null`. */
	onSnoozeWorkspace?: (workspaceId: string, until: string | null) => Promise<void>;
	/** Puts the task's failing PR checks, with their logs, in its agent's composer. */
	onSendChecksToAgent?: (workspaceId: string) => void;
	onRenameWorkspace?: (workspaceId: string, name: string) => void | Promise<void>;
	onCompleteWorkspace?: (workspaceId: string) => void | Promise<void>;
	onRestoreWorkspace?: (workspaceId: string) => void;
	onDeleteWorkspace?: (
		workspaceId: string,
		options?: {
			deleteRemoteBranch?: boolean;
			expectedRemoteTarget?: WorkspaceRemoteBranchDeletionTarget | null;
			expectedRemoteTargets?: WorkspaceRemoteBranchDeletionTarget[];
		},
	) => void | Promise<void>;
	onDeleteProject?: (input: {
		repositoryId: string;
		workspaceIds: string[];
	}) => Promise<void> | void;
	onUpdateProjectIdentity?: (input: {
		repositoryId: string;
		displayName: string | null;
		icon: string | null;
		color: string | null;
		logo: string | null;
	}) => Promise<void>;
	onSetProjectPinned?: (repositoryId: string, pinned: boolean) => Promise<void>;
	onSetWorkspacePinned?: (workspaceId: string, pinned: boolean) => Promise<void>;
	selectedWorkspaceId: string | null;
	workspaces: WorkspaceSummary[];
};

type ProjectRemovalTarget = {
	repositoryId: string;
	label: string;
	rootPath: string | null;
	workspaceCount: number;
	workspaceIds: string[];
};

export const WorkspacesSidebar = memo(function WorkspacesSidebar({
	collapsed,
	isCreatingWorkspace = false,
	showAgentStates = true,
	showCompletedDiskUsage = true,
	sessionQueryScope = "local",
	onSelectWorkspace: selectWorkspace,
	onNewTask,
	newTaskActive = false,
	onCreateWorkspace,
	onCloneWorkspace,
	onCreateWorkspaceFromProject,
	onOpenBranchFromProject,
	repositories,
	skillCount = 0,
	appUpdate = null,
	isInstallingUpdate = false,
	onInstallUpdate,
	onOpenSettings,
	onOpenSkills,
	onOpenUsage,
	onOpenNotes,
	onOpenFeedback,
	notesCount = 0,
	onOpenHelp,
	onOpenPullRequests,
	pullRequestsActive = false,
	agents = EMPTY_AGENTS,
	activeAgentId = null,
	onOpenAgent,
	onToggleCollapsed,
	onArchiveWorkspace,
	onSnoozeWorkspace,
	onSendChecksToAgent,
	onRenameWorkspace,
	onCompleteWorkspace,
	onRestoreWorkspace,
	onDeleteWorkspace,
	onDeleteProject,
	onUpdateProjectIdentity,
	onSetProjectPinned,
	onSetWorkspacePinned,
	selectedWorkspaceId,
	workspaces,
}: WorkspacesSidebarProps) {
	const { t, i18n } = useTranslation("common");
	// Snoozes end on their own: re-evaluate twice a minute.
	const [now, setNow] = useState(() => Date.now());
	useEffect(() => {
		const timer = window.setInterval(() => setNow(Date.now()), 30_000);
		return () => window.clearInterval(timer);
	}, []);
	// Opening a snoozed or woken task ends its snooze.
	const onSelectWorkspace = useCallback(
		(workspaceId: string) => {
			const workspace = workspaces.find((candidate) => candidate.id === workspaceId);
			if (workspace?.snoozedUntil && onSnoozeWorkspace) {
				void onSnoozeWorkspace(workspaceId, null).catch(() => undefined);
			}
			selectWorkspace(workspaceId);
		},
		[onSnoozeWorkspace, selectWorkspace, workspaces],
	);
	const workspaceAgentActivities = useWorkspaceAgentActivities(workspaces, {
		enabled: showAgentStates,
		scope: sessionQueryScope,
	});
	const workspaceProviderIds = useWorkspaceProviderIds(workspaces, {
		enabled: showAgentStates,
		scope: sessionQueryScope,
	});
	const runningActivities = useMemo(
		() => runningWorkspaceActivities(workspaces, workspaceAgentActivities),
		[workspaceAgentActivities, workspaces],
	);
	const seenResults = useSeenWorkspaceResults();
	const blockersQuery = useWorkspaceBlockers(showAgentStates && sessionQueryScope === "local");
	const workspaceBlockers = useMemo(
		() => blockersByWorkspace(blockersQuery.data ?? []),
		[blockersQuery.data],
	);
	const attentionItems = useMemo(
		() =>
			attentionWorkspaceItems(workspaces, workspaceAgentActivities, (workspaceId, completedAt) =>
				isWorkspaceResultUnread(completedAt, seenResults.seen[workspaceId], seenResults.baseline),
				workspaceBlockers,
				now,
			),
		[now, seenResults, workspaceAgentActivities, workspaceBlockers, workspaces],
	);
	const unseenResultWorkspaceIds = useMemo(
		() =>
			new Set(
				attentionItems
					.filter((item) => item.reason === "completed" || item.reason === "aborted")
					.map((item) => item.workspace.id),
			),
		[attentionItems],
	);
	// Opening a task is what marks its latest result as seen.
	const selectedActivity = selectedWorkspaceId
		? workspaceAgentActivities[selectedWorkspaceId]
		: undefined;
	const selectedFinishedAt =
		selectedActivity?.state === "completed" || selectedActivity?.state === "aborted"
			? selectedActivity.completedAt
			: null;
	useEffect(() => {
		if (selectedWorkspaceId && selectedFinishedAt) {
			markWorkspaceResultSeen(selectedWorkspaceId, selectedFinishedAt);
		}
	}, [selectedFinishedAt, selectedWorkspaceId]);
	const [showAllAttentionItems, setShowAllAttentionItems] = useState(false);
	const scrollContainerRef = useRef<HTMLDivElement>(null);
	// Activities are rebuilt every render; key the order on the timestamps only
	// so the grouped rows (and the virtual list) change only when one moves.
	const lastInteractionSignature = Object.entries(workspaceAgentActivities)
		.map(([id, activity]) => `${id}=${activity.lastInteractionAt ?? ""}`)
		.join("|");
	const lastInteractionAt = useMemo(
		() =>
			Object.fromEntries(
				lastInteractionSignature
					.split("|")
					.filter(Boolean)
					.map((entry) => {
						const separator = entry.indexOf("=");
						return [entry.slice(0, separator), entry.slice(separator + 1) || null];
					}),
			),
		[lastInteractionSignature],
	);
	const { activeGroups, waitingRows, completedRows } = useMemo(
		() => projectWorkspaceRailGroups(workspaces, repositories, lastInteractionAt, now),
		[lastInteractionAt, repositories, workspaces],
	);
	// A task moved into Em espera/Concluídos leaves its project group; the
	// destination header glows once so the move reads as "went there", not "gone".
	const [flashSectionId, setFlashSectionId] = useState<string | null>(null);
	const previousSectionMembershipRef = useRef<{
		workspaceIds: Set<string>;
		sections: Record<string, Set<string>>;
	} | null>(null);
	useEffect(() => {
		const sections: Record<string, Set<string>> = {
			[WAITING_SECTION_ID]: new Set(waitingRows.map((row) => row.id)),
			[COMPLETED_SECTION_ID]: new Set(completedRows.map((row) => row.id)),
		};
		const previous = previousSectionMembershipRef.current;
		previousSectionMembershipRef.current = {
			workspaceIds: new Set(workspaces.map((workspace) => workspace.id)),
			sections,
		};
		if (!previous) {
			return;
		}
		// Only moves count: a task that was already listed elsewhere, not one
		// that just finished loading.
		const movedInto = Object.keys(sections).find((sectionId) =>
			[...sections[sectionId]!].some(
				(id) => previous.workspaceIds.has(id) && !previous.sections[sectionId]?.has(id),
			),
		);
		if (!movedInto) {
			return;
		}
		setFlashSectionId(movedInto);
	}, [completedRows, waitingRows, workspaces]);
	useEffect(() => {
		if (!flashSectionId) {
			return;
		}
		const timeout = window.setTimeout(() => setFlashSectionId(null), 1600);
		return () => window.clearTimeout(timeout);
	}, [flashSectionId]);
	// Projects reorder by recency, but never while the pointer is over the rail.
	const [frozenProjectOrder, setFrozenProjectOrder] = useState<string[] | null>(null);
	const displayedGroups = useMemo(
		() =>
			frozenProjectOrder
				? freezeRailGroupOrder(activeGroups, frozenProjectOrder)
				: activeGroups,
		[activeGroups, frozenProjectOrder],
	);
	const { visibleGroups: railGroups, idleGroups } = useMemo(
		() => splitIdleRailGroups(displayedGroups, new Set(frozenProjectOrder ?? [])),
		[displayedGroups, frozenProjectOrder],
	);
	const repositoriesBySourceKey = useMemo(
		() =>
			new Map(
				repositories.map((repository) => [repository.rootPath.trim(), repository]),
			),
		[repositories],
	);
	const groupSignals = useMemo(
		() =>
			new Map(
				activeGroups.map((group) => [
					group.id,
					workspaceRailGroupSignal(group.rows, workspaceAgentActivities),
				]),
			),
		[activeGroups, workspaceAgentActivities],
	);
	const selectedProjectSourceKey = useMemo(
		() => workspaces.find((workspace) => workspace.id === selectedWorkspaceId)?.rootPath?.trim(),
		[workspaces, selectedWorkspaceId],
	);
	const projectLabelsBySourceKey = useMemo(
		() => new Map(activeGroups.map((group) => [group.sourceKey, group.label])),
		[activeGroups],
	);
	const visibleAttentionItems = showAllAttentionItems
		? attentionItems
		: attentionItems.slice(0, ATTENTION_PREVIEW_LIMIT);
	const hiddenAttentionItemCount = attentionItems.length - visibleAttentionItems.length;

	useEffect(() => {
		if (attentionItems.length <= ATTENTION_PREVIEW_LIMIT) {
			setShowAllAttentionItems(false);
		}
	}, [attentionItems.length]);
	const [projectRemovalTarget, setProjectRemovalTarget] = useState<ProjectRemovalTarget | null>(
		null,
	);
	const [projectEditTarget, setProjectEditTarget] = useState<Repository | null>(null);
	const [isRemovingProject, setIsRemovingProject] = useState(false);
	const [workspaceDeletionTarget, setWorkspaceDeletionTarget] =
		useState<WorkspaceSummary | null>(null);
	const [deleteRemoteBranch, setDeleteRemoteBranch] = useState(false);
	const [isDeletingWorkspace, setIsDeletingWorkspace] = useState(false);
	const [isCleanupOpen, setIsCleanupOpen] = useState(false);
	const completedScan = useCompletedCleanupScan(completedRows, showCompletedDiskUsage);
	const hasCompletedRows = showCompletedDiskUsage && completedRows.length > 0;
	const completedScanData = hasCompletedRows ? completedScan.data : undefined;
	const completedDiskUsage = useMemo((): {
		status: "idle" | "loading" | "ready" | "error";
		totalBytes: number;
		bytesByWorkspaceId: Record<string, number>;
	} => {
		if (!hasCompletedRows) {
			return { status: "idle", totalBytes: 0, bytesByWorkspaceId: {} };
		}
		if (!completedScanData) {
			return {
				status: completedScan.isError ? "error" : "loading",
				totalBytes: 0,
				bytesByWorkspaceId: {},
			};
		}
		return {
			status: "ready",
			totalBytes: completedScanData.totalBytes,
			bytesByWorkspaceId: Object.fromEntries(
				completedScanData.workspaces.map((entry) => [entry.workspaceId, entry.bytes]),
			),
		};
	}, [completedScan.isError, completedScanData, hasCompletedRows]);

	// The reporter points at completed tasks once they pass the person's limit.
	const cleanupAlertState = useCleanupAlert();
	const completedTotalBytes = !showCompletedDiskUsage
		? null
		: completedRows.length === 0
			? 0
			: (completedScanData?.totalBytes ?? null);
	useEffect(() => {
		if (completedTotalBytes !== null) {
			syncCleanupAlert(completedTotalBytes);
		}
	}, [completedTotalBytes]);
	const completedCleanupSummaryValue = useMemo(
		() =>
			completedCleanupSummary(
				completedScanData ? completedCleanupRows(completedRows, completedScanData.workspaces) : [],
			),
		[completedRows, completedScanData],
	);
	const [cleanupProgress, setCleanupProgress] = useState<{
		done: number;
		total: number;
	} | null>(null);
	const cleanupAlertDue =
		completedTotalBytes !== null && isCleanupAlertDue(cleanupAlertState, completedTotalBytes);
	const handleCleanSafeCompleted = useCallback(async () => {
		if (!onDeleteWorkspace || cleanupProgress) {
			return;
		}
		setCleanupProgress({ done: 0, total: 0 });
		try {
			// Checked again right before deleting: the cached verdict can be minutes old.
			const fresh = await workspaceCleanupScan({
				workspaceIds: workspaceDiskUsageIds(completedRows),
			});
			const targets = completedCleanupRows(completedRows, fresh.workspaces).filter(
				isPreselectedForCleanup,
			);
			if (targets.length === 0) {
				toast.info(t("agents.cleanup.bubble.nothingSafe"));
				return;
			}
			setCleanupProgress({ done: 0, total: targets.length });
			const { freedBytes, failed } = await deleteCompletedTasks(
				targets,
				(workspaceId) => onDeleteWorkspace(workspaceId, { deleteRemoteBranch: false }),
				({ done, total }) => setCleanupProgress({ done, total }),
			);
			// What is left was judged unsafe; do not point at it again right away.
			dismissCleanupAlert(Math.max(0, fresh.totalBytes - freedBytes));
			if (failed > 0) {
				toast.error(t("sidebar.cleanup.partial", { count: failed }));
			} else {
				toast.success(
					t("sidebar.cleanup.freed", {
						size: formatDiskBytes(freedBytes, i18n.resolvedLanguage),
					}),
				);
			}
		} catch (error) {
			console.warn("[dcc] failed to clean up completed tasks", error);
			toast.error(t("sidebar.cleanup.scanFailed"));
		} finally {
			setCleanupProgress(null);
		}
	}, [cleanupProgress, completedRows, i18n.resolvedLanguage, onDeleteWorkspace, t]);
	// The reporter delivers this above the composer; the sidebar keeps the data
	// and the cleanup list.
	const reporterCleanup = useMemo<ReporterCleanupOffer | null>(
		() =>
			onDeleteWorkspace && completedTotalBytes !== null && (cleanupAlertDue || cleanupProgress)
				? {
						alert: {
							totalBytes: completedTotalBytes,
							safeCount: completedCleanupSummaryValue.safeCount,
							safeBytes: completedCleanupSummaryValue.safeBytes,
							progress: cleanupProgress,
						},
						onReview: () => setIsCleanupOpen(true),
						onCleanSafe: () => {
							void handleCleanSafeCompleted();
						},
						onDismiss: () => dismissCleanupAlert(completedTotalBytes),
					}
				: null,
		[
			cleanupAlertDue,
			cleanupProgress,
			completedCleanupSummaryValue,
			completedTotalBytes,
			handleCleanSafeCompleted,
			onDeleteWorkspace,
		],
	);
	useEffect(() => {
		publishReporterCleanup(reporterCleanup);
	}, [reporterCleanup]);
	useEffect(() => () => publishReporterCleanup(null), []);

	// The reporter's daily recap, also said above the composer.
	const dailyRecap = useDailyRecap();
	const recapText = useRecapBubbleText(
		isRecapBubbleVisible(dailyRecap) ? dailyRecap.current : null,
		attentionItems.filter((item) => RECAP_BLOCKING_REASONS.has(item.reason)).length,
	);
	const reporterId = agents.find((agent) => agent.preset === "chronicler")?.id ?? null;
	const reporterRecap = useMemo<ReporterRecapNews | null>(
		() =>
			recapText && reporterId && onOpenAgent
				? {
						text: recapText,
						onOpen: () => onOpenAgent(reporterId),
						onDismiss: dismissDailyRecapBubble,
					}
				: null,
		[onOpenAgent, recapText, reporterId],
	);
	useEffect(() => {
		publishReporterRecap(reporterRecap);
	}, [reporterRecap]);
	useEffect(() => () => publishReporterRecap(null), []);

	const workspaceDeletionBytes = useMemo(() => {
		if (!workspaceDeletionTarget || completedDiskUsage.status !== "ready") {
			return null;
		}
		const memberIds = workspaceDiskUsageIds([workspaceDeletionTarget]);
		if (!memberIds.some((workspaceId) => workspaceId in completedDiskUsage.bytesByWorkspaceId)) {
			return null;
		}
		return memberIds.reduce(
			(total, workspaceId) =>
				total + (completedDiskUsage.bytesByWorkspaceId[workspaceId] ?? 0),
			0,
		);
	}, [completedDiskUsage, workspaceDeletionTarget]);

	const [sectionOpenState, setSectionOpenState] = useState(() => ({
		...createInitialRailSectionState(activeGroups),
		...readStoredRailSectionState(),
	}));

	useEffect(() => {
		setSectionOpenState((current) => {
			const next: Record<string, boolean> = {};
			let changed = false;

			for (const group of activeGroups) {
				const nextValue = current[group.id] ?? true;
				next[group.id] = nextValue;
				if (current[group.id] !== nextValue) {
					changed = true;
				}
			}

			for (const sectionId of [PINNED_SECTION_ID, RECENT_SECTION_ID]) {
				const sectionValue = current[sectionId] ?? true;
				next[sectionId] = sectionValue;
				if (current[sectionId] !== sectionValue) {
					changed = true;
				}
			}

			const waitingValue = current[WAITING_SECTION_ID] ?? false;
			next[WAITING_SECTION_ID] = waitingValue;
			if (current[WAITING_SECTION_ID] !== waitingValue) {
				changed = true;
			}

			const completedValue = current[COMPLETED_SECTION_ID] ?? false;
			next[COMPLETED_SECTION_ID] = completedValue;
			if (current[COMPLETED_SECTION_ID] !== completedValue) {
				changed = true;
			}

			if (Object.keys(current).length !== Object.keys(next).length) {
				changed = true;
			}

			return changed ? next : current;
		});
	}, [activeGroups]);

	useEffect(() => {
		writeStoredRailSectionState(sectionOpenState);
	}, [sectionOpenState]);

	const lastAutoExpandedIdRef = useRef<string | null>(null);
	useEffect(() => {
		if (!selectedWorkspaceId || selectedWorkspaceId === lastAutoExpandedIdRef.current) {
			return;
		}

		const selectedSectionId = findSelectedRailSectionId(
			selectedWorkspaceId,
			activeGroups,
			waitingRows,
			completedRows,
		);

		if (!selectedSectionId) {
			return;
		}

		lastAutoExpandedIdRef.current = selectedWorkspaceId;
		const selectedGroup = activeGroups.find((group) => group.id === selectedSectionId);
		const parentSectionId =
			selectedGroup && activeGroups.some((group) => group.pinnedAt)
				? selectedGroup.pinnedAt
					? PINNED_SECTION_ID
					: RECENT_SECTION_ID
				: null;
		setSectionOpenState((current) =>
			current[selectedSectionId] && (!parentSectionId || current[parentSectionId] !== false)
				? current
				: {
						...current,
						[selectedSectionId]: true,
						...(parentSectionId ? { [parentSectionId]: true } : {}),
					},
		);
	}, [activeGroups, completedRows, selectedWorkspaceId, waitingRows]);

	const flatItems = useMemo(() => {
		const items: VirtualItem[] = [];
		const visibleGroups = railGroups;
		const hasPinnedGroups = visibleGroups.some((group) => group.pinnedAt);

		for (let gi = 0; gi < visibleGroups.length; gi++) {
			const group = visibleGroups[gi]!;
			const startsPinned = hasPinnedGroups && gi === 0;
			const startsRecent =
				hasPinnedGroups && !group.pinnedAt && Boolean(visibleGroups[gi - 1]?.pinnedAt);
			if (startsPinned || startsRecent) {
				const sectionId = startsPinned ? PINNED_SECTION_ID : RECENT_SECTION_ID;
				if (startsRecent) {
					items.push({ kind: "group-gap", size: GROUP_GAP });
				}
				items.push({
					kind: "section-label",
					sectionId,
					label: t(startsPinned ? "sidebar.pinnedSection" : "sidebar.recentSection"),
					groupIds: visibleGroups
						.filter((candidate) => Boolean(candidate.pinnedAt) === startsPinned)
						.map((candidate) => candidate.id),
				});
			}
			if (
				hasPinnedGroups &&
				sectionOpenState[group.pinnedAt ? PINNED_SECTION_ID : RECENT_SECTION_ID] === false
			) {
				continue;
			}
			if (!startsPinned && !startsRecent && gi > 0) {
				const previousGroup = visibleGroups[gi - 1]!;
				items.push({
					kind: "group-gap",
					size: getGroupGapSize(
						previousGroup.rows.length > 0,
						group.rows.length > 0,
					),
				});
			}

			const canCollapse = group.rows.length > 0;
			items.push({
				kind: "group-header",
				groupId: group.id,
				label: group.label,
				sourceKey: group.sourceKey,
				rowCount: group.rows.length,
				canCollapse,
				headerVariant: "project",
			});

			if (sectionOpenState[group.id] !== false && group.rows.length > 0) {
				for (const row of group.rows) {
					items.push({
						kind: "row",
						groupId: group.id,
						workspace: row,
					});
				}
			}
		}

		let previousHasRows = (visibleGroups.at(-1)?.rows.length ?? 0) > 0;
		// Projects without tasks wait behind one line at the end of Recent.
		const recentCollapsed =
			hasPinnedGroups &&
			visibleGroups.some((group) => !group.pinnedAt) &&
			sectionOpenState[RECENT_SECTION_ID] === false;
		if (idleGroups.length > 0 && !recentCollapsed) {
			const isOpen = sectionOpenState[IDLE_SECTION_ID] ?? visibleGroups.length === 0;
			if (visibleGroups.length > 0) {
				items.push({ kind: "group-gap", size: getGroupGapSize(previousHasRows, false) });
			}
			items.push({ kind: "idle-toggle", count: idleGroups.length, isOpen });
			if (isOpen) {
				for (const group of idleGroups) {
					items.push({ kind: "group-gap", size: EMPTY_GROUP_GAP });
					items.push({
						kind: "group-header",
						groupId: group.id,
						label: group.label,
						sourceKey: group.sourceKey,
						rowCount: 0,
						canCollapse: false,
						headerVariant: "project",
					});
				}
			}
			previousHasRows = false;
		}

		const specialSections = [
			{
				id: WAITING_SECTION_ID,
				label: t("sidebar.waiting"),
				rows: waitingRows,
				headerVariant: "waiting" as const,
			},
			{
				id: COMPLETED_SECTION_ID,
				label: t("sidebar.completed"),
				rows: completedRows,
				headerVariant: "completed" as const,
			},
		];
		for (const section of specialSections) {
			// An empty section says nothing; it shows up with the first task moved in.
			if (section.rows.length === 0) {
				continue;
			}
			items.push({
				kind: "group-gap",
				size: getGroupGapSize(previousHasRows, section.rows.length > 0),
			});
			items.push({
				kind: "group-header",
				groupId: section.id,
				label: section.label,
				rowCount: section.rows.length,
				canCollapse: section.rows.length > 0,
				headerVariant: section.headerVariant,
			});

			if (sectionOpenState[section.id] && section.rows.length > 0) {
				for (const row of section.rows) {
					items.push({
						kind: "row",
						groupId: section.id,
						workspace: row,
					});
				}
			}
			previousHasRows = section.rows.length > 0;
		}

		items.push({ kind: "bottom-padding" });
		return items;
	}, [completedRows, idleGroups, railGroups, sectionOpenState, t, waitingRows]);

	const virtualizer = useVirtualizer({
		count: flatItems.length,
		getScrollElement: () => scrollContainerRef.current,
		estimateSize: (index) => {
			const item = flatItems[index]!;
			switch (item.kind) {
				case "group-header":
					return HEADER_HEIGHT;
				case "section-label":
					return SECTION_LABEL_HEIGHT;
				case "idle-toggle":
					return IDLE_TOGGLE_HEIGHT;
				case "row":
					return isCompactRailSection(item.groupId) ? COMPACT_ROW_HEIGHT : ROW_HEIGHT;
				case "group-gap":
					return item.size;
				case "bottom-padding":
					return BOTTOM_PADDING;
			}
		},
		getItemKey: (index) => {
			const item = flatItems[index]!;
			switch (item.kind) {
				case "group-header":
					return `header-${item.groupId}`;
				case "section-label":
					return `section-${item.sectionId}`;
				case "idle-toggle":
					return "idle-toggle";
				case "row":
					return `row-${item.groupId}-${item.workspace.id}`;
				case "group-gap":
					return `gap-${index}`;
				case "bottom-padding":
					return "bottom-padding";
			}
		},
		overscan: 12,
	});

	useLayoutEffect(() => {
		if (!selectedWorkspaceId) {
			return;
		}

		const targetIndex = flatItems.findIndex(
			(item) => item.kind === "row" && item.workspace.id === selectedWorkspaceId,
		);
		if (targetIndex === -1) {
			return;
		}

		virtualizer.scrollToIndex(targetIndex, { align: "auto" });
	}, [selectedWorkspaceId, sectionOpenState, flatItems, virtualizer]);

	const toggleSection = useCallback((groupId: string) => {
		setSectionOpenState((current) => ({
			...current,
			[groupId]: !current[groupId],
		}));
	}, []);

	const openProjectRemovalDialog = useCallback(
		(sourceKey: string, label: string) => {
			const matchingWorkspaces = workspaces.filter(
				(workspace) => projectGroupingKey(workspace) === sourceKey,
			);
			const rootPath =
				matchingWorkspaces.find((workspace) => workspace.rootPath?.trim())?.rootPath?.trim() ??
				matchingWorkspaces.find((workspace) => workspace.worktreePath?.trim())?.worktreePath?.trim() ??
				repositoriesBySourceKey.get(sourceKey)?.rootPath?.trim() ??
				null;

			const repository = repositoriesBySourceKey.get(sourceKey) ?? null;
			setProjectRemovalTarget({
				repositoryId: repository?.id ?? sourceKey,
				label,
				rootPath,
				workspaceCount: matchingWorkspaces.length,
				workspaceIds: matchingWorkspaces.map((workspace) => workspace.id),
			});
		},
		[repositoriesBySourceKey, workspaces],
	);

	const handleConfirmProjectRemoval = useCallback(async () => {
		if (!projectRemovalTarget || !onDeleteProject) {
			return;
		}

		setIsRemovingProject(true);
		try {
			await onDeleteProject({
				repositoryId: projectRemovalTarget.repositoryId,
				workspaceIds: projectRemovalTarget.workspaceIds,
			});
			setProjectRemovalTarget(null);
		} catch (error) {
			console.error("[dcc] remove project failed", {
				label: projectRemovalTarget.label,
				workspaceIds: projectRemovalTarget.workspaceIds,
				error,
			});
			toast.error(
				error instanceof Error ? error.message : t("sidebar.removeProjectError"),
			);
		} finally {
			setIsRemovingProject(false);
		}
	}, [onDeleteProject, projectRemovalTarget, t]);

	const openWorkspaceDeletionDialog = useCallback(
		(workspaceId: string) => {
			const workspace =
				workspaces.find((candidate) => candidate.id === workspaceId) ?? null;
			if (!workspace) {
				return;
			}
			setWorkspaceDeletionTarget(workspace);
			setDeleteRemoteBranch(false);
		},
		[workspaces],
	);

	const handleConfirmWorkspaceDeletion = useCallback(async () => {
		if (!workspaceDeletionTarget || !onDeleteWorkspace) {
			return;
		}
		setIsDeletingWorkspace(true);
		try {
			const remoteTargets = workspaceDeletionTarget.remoteDeletionTargets ?? [];
			await onDeleteWorkspace(workspaceDeletionTarget.id, {
				deleteRemoteBranch,
				expectedRemoteTarget: remoteTargets[0] ?? null,
				expectedRemoteTargets: remoteTargets,
			});
			setWorkspaceDeletionTarget(null);
		} catch (error) {
			const message =
				error instanceof Error
					? error.message
					: typeof error === "string"
						? error
						: "";
			toast.error(
				message.includes("Undo recovery is still active")
					? t("sidebar.deleteWorkspaceUndoRecoveryActive")
					: message || t("sidebar.deleteWorkspaceError"),
			);
			// The backend may already have removed a remote branch or worktree.
			// Closing forces the next confirmation to use the refreshed target.
			setWorkspaceDeletionTarget(null);
		} finally {
			setIsDeletingWorkspace(false);
		}
	}, [
		deleteRemoteBranch,
		onDeleteWorkspace,
		t,
		workspaceDeletionTarget,
	]);

	const renderItem = useCallback(
		(item: VirtualItem) => {
			if (item.kind === "group-gap" || item.kind === "bottom-padding") {
				return null;
			}

			if (item.kind === "section-label") {
				const isOpen = sectionOpenState[item.sectionId] !== false;
				// Collapsed, the label carries what its projects would have said.
				const sectionSignals = item.groupIds.map((groupId) => groupSignals.get(groupId) ?? null);
				const sectionSignal = sectionSignals.includes("attention")
					? "attention"
					: sectionSignals.includes("running")
						? "running"
						: null;
				return (
					<button
						type="button"
						data-rail-section={item.sectionId}
						aria-expanded={isOpen}
						onClick={() => toggleSection(item.sectionId)}
						className="group/dccRailSection flex h-full w-full cursor-pointer select-none items-end gap-1 px-3 pb-1 text-muted-foreground/70 transition-colors hover:text-foreground"
					>
						{/* Sized on the span: see the button font reset note on project headers. */}
						<span className="flex items-center gap-1 text-[10px] font-semibold uppercase tracking-[0.08em]">
							<ChevronRight
								className={cn(
									"size-2.5 shrink-0 transition-transform duration-150",
									isOpen && "rotate-90",
								)}
								strokeWidth={2.4}
								aria-hidden
							/>
							{item.sectionId === PINNED_SECTION_ID ? (
								<Pin className="size-2.5 rotate-[-12deg]" strokeWidth={2.2} aria-hidden />
							) : null}
							{item.label}
						</span>
						{!isOpen ? (
							<span className="ml-1 flex items-center gap-1.5 pb-px text-[10px] font-medium tabular-nums">
								{item.groupIds.length}
								{sectionSignal ? (
									<span
										role="img"
										data-group-signal={sectionSignal}
										aria-label={t(`sidebar.groupSignal.${sectionSignal}`)}
										title={t(`sidebar.groupSignal.${sectionSignal}`)}
										className={cn(
											"size-[6px] rounded-full",
											sectionSignal === "attention"
												? "bg-amber-500"
												: "animate-pulse bg-emerald-500",
										)}
									/>
								) : null}
							</span>
						) : null}
					</button>
				);
			}

			if (item.kind === "idle-toggle") {
				return (
					<button
						type="button"
						data-rail-section={IDLE_SECTION_ID}
						aria-expanded={item.isOpen}
						onClick={() =>
							setSectionOpenState((current) => ({
								...current,
								[IDLE_SECTION_ID]: !item.isOpen,
							}))
						}
						className="flex h-full w-full cursor-pointer select-none items-center gap-1.5 px-3 text-muted-foreground/70 transition-colors hover:text-foreground"
					>
						{/* Sized on the span: see the button font reset note on project headers. */}
						<span className="flex items-center gap-1.5 text-[12px]">
							<ChevronRight
								className={cn(
									"size-3 shrink-0 transition-transform duration-150",
									item.isOpen && "rotate-90",
								)}
								strokeWidth={2.2}
								aria-hidden
							/>
							{t("sidebar.idleProjects", { count: item.count })}
						</span>
					</button>
				);
			}

			if (item.kind === "group-header") {
				const isOpen =
					item.headerVariant === "project"
						? (sectionOpenState[item.groupId] ?? true)
						: (sectionOpenState[item.groupId] ?? false);
				const isEmptyGroup = item.rowCount === 0;
				const groupSignal =
					item.headerVariant === "project"
						? (groupSignals.get(item.groupId) ?? null)
						: null;
				const repository =
					item.headerVariant === "project" && item.sourceKey
						? repositoriesBySourceKey.get(item.sourceKey) ?? null
						: null;
				const canOpenProjectBranch =
					item.headerVariant === "project" &&
					repository !== null &&
					Boolean(onOpenBranchFromProject) &&
					!isCreatingWorkspace;
				const canRemoveProject =
					item.headerVariant === "project" &&
					Boolean(item.sourceKey) &&
					Boolean(onDeleteProject) &&
					!isRemovingProject;
				const canEditProject =
					item.headerVariant === "project" &&
					repository !== null &&
					Boolean(onUpdateProjectIdentity);
				const canPinProject =
					item.headerVariant === "project" &&
					repository !== null &&
					Boolean(onSetProjectPinned);
				const canManageProject =
					canEditProject ||
					canPinProject ||
					canRemoveProject;
				const canCleanUpCompleted =
					item.headerVariant === "completed" &&
					item.rowCount > 0 &&
					showCompletedDiskUsage &&
					Boolean(onDeleteWorkspace);

				return (
					<div
						className={cn(
							"dcc-project-heading group/dccRailHeader flex items-center gap-1 rounded-md pr-1 transition-colors hover:bg-accent/50",
						)}
						data-empty-group={isEmptyGroup ? "true" : "false"}
						data-flash={flashSectionId === item.groupId ? "true" : undefined}
						data-current={item.headerVariant === "project" && item.sourceKey === selectedProjectSourceKey ? "true" : "false"}
					>
						<button
							type="button"
							className="flex min-w-0 flex-1 cursor-pointer select-none items-center justify-between rounded-md px-2 py-1.5 text-[12px] font-semibold tracking-[0.005em] text-foreground/75 transition-colors hover:text-foreground group-hover/dccRailHeader:text-foreground"
							disabled={!item.canCollapse}
							aria-expanded={item.canCollapse ? isOpen : undefined}
							onClick={() => toggleSection(item.groupId)}
						>
							<span className="flex min-w-0 items-center gap-1.5">
								<ChevronRight
									className={cn(
										"size-3 shrink-0 text-muted-foreground transition-transform duration-150",
										isOpen && "rotate-90",
										!item.canCollapse && "opacity-0",
									)}
									strokeWidth={2.2}
									aria-hidden
								/>
								{item.headerVariant === "waiting" ? (
									<Clock3
										className="size-[13px] shrink-0 text-muted-foreground/75"
										strokeWidth={1.9}
										aria-hidden
									/>
								) : item.headerVariant === "completed" ? (
									<CircleCheckBig
										className="size-[13px] shrink-0 text-muted-foreground/75"
										strokeWidth={1.9}
										aria-hidden
									/>
								) : repository ? (
									<ProjectIdentityGlyph
										icon={projectIconValue(repository)}
										color={repository.color}
										seed={repository.rootPath}
										active={groupSignal === "running"}
										size="sm"
										className="size-[18px]"
									/>
								) : (
									<ProjectGroupGlyph className="size-[13px] text-muted-foreground/75" />
								)}
								{/* Sized on the span: an unlayered `button { font: inherit }` reset in
								    color-theme.css overrides text utilities on the button itself. */}
								<span className="dcc-project-label truncate text-[13px] font-medium leading-5">
									{item.label}
								</span>
								{item.headerVariant === "completed" &&
								completedDiskUsage.status === "ready" ? (
									<span
										className="shrink-0 text-[10px] font-normal tabular-nums text-muted-foreground/70"
										title={t("sidebar.completedDiskUsageTitle", {
											size: formatDiskBytes(
												completedDiskUsage.totalBytes,
												i18n.resolvedLanguage,
											),
										})}
									>
										·{" "}
										{formatDiskBytes(
											completedDiskUsage.totalBytes,
											i18n.resolvedLanguage,
										)}
									</span>
								) : null}
							</span>

							{item.rowCount > 0 ? (
								<span className="ml-1.5 flex shrink-0 items-center gap-1.5">
									{groupSignal ? (
										<span
											role="img"
											data-group-signal={groupSignal}
											aria-label={t(`sidebar.groupSignal.${groupSignal}`)}
											title={t(`sidebar.groupSignal.${groupSignal}`)}
											className={cn(
												"size-[6px] rounded-full",
												groupSignal === "attention"
													? "bg-amber-500"
													: "animate-pulse bg-emerald-500",
											)}
										/>
									) : null}
									<span
										className="flex h-[17px] min-w-[17px] items-center justify-center rounded-full bg-foreground/[0.06] px-1 text-[9.5px] font-semibold tabular-nums text-muted-foreground"
										title={t("sidebar.workspaceCount", {
											count: item.rowCount,
										})}
									>
										{item.rowCount}
									</span>
								</span>
							) : null}
						</button>

						{canCleanUpCompleted ? (
							<Tooltip>
								<TooltipTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="icon-xs"
										aria-label={t("sidebar.cleanup.action")}
										className="shrink-0 text-muted-foreground opacity-0 transition-opacity hover:text-foreground group-hover/dccRailHeader:opacity-100 group-focus-within/dccRailHeader:opacity-100"
										onClick={(event) => {
											event.stopPropagation();
											setIsCleanupOpen(true);
										}}
									>
										<BrushCleaning className="size-3.5" strokeWidth={2} aria-hidden />
									</Button>
								</TooltipTrigger>
								<TooltipContent side="top">{t("sidebar.cleanup.action")}</TooltipContent>
							</Tooltip>
						) : null}

						{canOpenProjectBranch ? (
							<Tooltip>
								<TooltipTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="icon-xs"
										aria-label={t("sidebar.openBranchFromProject", {
											label: item.label,
										})}
										className="shrink-0 text-muted-foreground opacity-0 transition-opacity hover:text-foreground group-hover/dccRailHeader:opacity-100 group-focus-within/dccRailHeader:opacity-100"
										onClick={(event) => {
											event.stopPropagation();
											onOpenBranchFromProject?.({
												projectId: repository.projectId,
												workspaceRoot: repository.rootPath,
												baseBranch: repository.baseBranch,
												label: repositoryDisplayName(repository),
											});
										}}
									>
										<GitBranch className="size-3.5" strokeWidth={2} aria-hidden />
									</Button>
								</TooltipTrigger>
								<TooltipContent side="top">
									{t("sidebar.openBranch")}
								</TooltipContent>
							</Tooltip>
						) : null}

						{canManageProject ? (
							<DropdownMenu>
								<DropdownMenuTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="icon-xs"
										aria-label={t("sidebar.projectActions")}
										className="shrink-0 text-muted-foreground opacity-0 transition-opacity hover:text-foreground group-hover/dccRailHeader:opacity-100 group-focus-within/dccRailHeader:opacity-100"
										onClick={(event) => {
											event.stopPropagation();
										}}
									>
										<MoreHorizontal className="size-4" strokeWidth={2} aria-hidden />
									</Button>
								</DropdownMenuTrigger>
								<DropdownMenuContent align="end" sideOffset={6}>
									{canPinProject ? (
										<DropdownMenuItem
											className="gap-2 text-[13px]"
											onSelect={(event) => {
												event.preventDefault();
												void onSetProjectPinned?.(
													repository.id,
													!repository.pinnedAt,
												).catch((error) =>
													toast.error(
														error instanceof Error
															? error.message
															: t("sidebar.pinProjectError"),
													),
												);
											}}
										>
											{repository.pinnedAt ? (
												<PinOff className="size-3.5" strokeWidth={1.9} aria-hidden />
											) : (
												<Pin className="size-3.5" strokeWidth={1.9} aria-hidden />
											)}
											{repository.pinnedAt
												? t("sidebar.unpinProject")
												: t("sidebar.pinProject")}
										</DropdownMenuItem>
									) : null}
									{canEditProject ? (
										<DropdownMenuItem
											className="gap-2 text-[13px]"
											onSelect={(event) => {
												event.preventDefault();
												setProjectEditTarget(repository);
											}}
										>
											<Settings2 className="size-3.5" strokeWidth={1.9} aria-hidden />
											{t("sidebar.editProject")}
										</DropdownMenuItem>
									) : null}
									{canRemoveProject ? (
										<DropdownMenuItem
											className="gap-2 text-[13px] text-destructive focus:text-destructive"
											onSelect={(event) => {
												event.preventDefault();
												openProjectRemovalDialog(item.sourceKey!, item.label);
											}}
										>
											<Trash2 className="size-3.5" strokeWidth={2} aria-hidden />
											{t("sidebar.removeProject")}
										</DropdownMenuItem>
									) : null}
								</DropdownMenuContent>
							</DropdownMenu>
						) : null}
					</div>
				);
			}

			const workspaceRepository = item.workspace.rootPath
				? repositoriesBySourceKey.get(item.workspace.rootPath.trim()) ?? null
				: null;

			return (
				<WorkspaceRailRowItem
					workspace={item.workspace}
					selected={selectedWorkspaceId === item.workspace.id}
					compact={isCompactRailSection(item.groupId)}
					activity={workspaceAgentActivities[item.workspace.id] ?? null}
					unseenResult={unseenResultWorkspaceIds.has(item.workspace.id)}
					pendingDelegatedReviews={
						workspaceBlockers
							.get(item.workspace.id)
							?.find((blocker) => blocker.kind === "delegated_edits_review")?.count ?? 0
					}
					providerId={workspaceProviderIds[item.workspace.id] ?? null}
					metadataEnabled={showAgentStates}
					projectLabel={
						workspaceRepository
							? repositoryDisplayName(workspaceRepository)
							: null
					}
					projectIcon={workspaceRepository ? projectIconValue(workspaceRepository) : null}
					projectColor={workspaceRepository?.color ?? null}
					projectSeed={workspaceRepository?.rootPath ?? null}
					onSelect={
						item.workspace.status === "archived" ||
						item.workspace.status === "completed"
							? undefined
							: onSelectWorkspace
					}
					onArchiveWorkspace={onArchiveWorkspace}
					onSnoozeWorkspace={onSnoozeWorkspace}
					onRenameWorkspace={onRenameWorkspace}
					onCompleteWorkspace={onCompleteWorkspace}
					onRestoreWorkspace={onRestoreWorkspace}
					onDeleteWorkspace={
						onDeleteWorkspace ? openWorkspaceDeletionDialog : undefined
					}
					onSetWorkspacePinned={onSetWorkspacePinned}
				/>
			);
		},
		[
			completedDiskUsage,
			flashSectionId,
			groupSignals,
			i18n.resolvedLanguage,
			isCreatingWorkspace,
			isRemovingProject,
			onArchiveWorkspace,
			onSnoozeWorkspace,
			onRenameWorkspace,
			onCompleteWorkspace,
			onOpenBranchFromProject,
			onDeleteProject,
			onUpdateProjectIdentity,
			onSetProjectPinned,
			onSetWorkspacePinned,
			onDeleteWorkspace,
			onRestoreWorkspace,
			onSelectWorkspace,
			openProjectRemovalDialog,
			openWorkspaceDeletionDialog,
			repositoriesBySourceKey,
			sectionOpenState,
			selectedProjectSourceKey,
			showCompletedDiskUsage,
			selectedWorkspaceId,
			t,
			toggleSection,
			unseenResultWorkspaceIds,
			workspaceAgentActivities,
			workspaceBlockers,
			showAgentStates,
		],
	);

	if (collapsed) {
		return (
			<div className="dcc-project-sidebar flex h-full min-h-0 flex-col items-center gap-2 overflow-hidden py-2">
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							onClick={onToggleCollapsed}
							aria-label={t("sidebar.expandSidebar")}
							className="text-muted-foreground hover:text-foreground"
						>
							<PanelRight className="size-4" strokeWidth={1.8} />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="right">{t("sidebar.expandSidebar")}</TooltipContent>
				</Tooltip>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							onClick={onNewTask}
							disabled={isCreatingWorkspace}
							aria-current={newTaskActive ? "page" : undefined}
							aria-label={t("sidebar.newWorkspace")}
							className={cn(
								"text-muted-foreground hover:text-foreground",
								newTaskActive && "bg-accent text-foreground",
							)}
						>
							<Plus className="size-4" strokeWidth={1.9} />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="right">{t("sidebar.newWorkspace")}</TooltipContent>
				</Tooltip>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							onClick={onOpenPullRequests}
							aria-current={pullRequestsActive ? "page" : undefined}
							aria-label={t("sidebar.pullRequests")}
							className={cn(
								"text-muted-foreground hover:text-foreground",
								pullRequestsActive && "bg-accent text-foreground",
							)}
						>
							<GitPullRequest className="size-4" strokeWidth={1.9} />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="right">{t("sidebar.pullRequests")}</TooltipContent>
				</Tooltip>

				{onOpenNotes && (
					<Tooltip>
						<TooltipTrigger asChild>
							<Button variant="ghost" size="icon-xs" onClick={onOpenNotes} aria-label={t("notes.headingShort")}>
								<StickyNote className="size-4 text-muted-foreground" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="right">{t("notes.headingShort")}</TooltipContent>
					</Tooltip>
				)}
				{onOpenAgent && (
					<AgentsSidebarSection
						collapsed
						agents={agents}
						activeAgentId={activeAgentId}
						onOpenAgent={onOpenAgent}
					/>
				)}
				{workspaces.length > 0 ? (
					<>
						<Tooltip>
							<TooltipTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="icon-xs"
									aria-label={t("sidebar.openProject")}
									className="text-muted-foreground hover:text-foreground"
									disabled={isCreatingWorkspace}
									onClick={onCreateWorkspace}
								>
									<FolderPlus className="size-4" strokeWidth={1.9} aria-hidden />
								</Button>
							</TooltipTrigger>
							<TooltipContent side="right">{t("sidebar.openProject")}</TooltipContent>
						</Tooltip>
						<WorkspaceRepoPicker
							repositories={repositories}
							isDisabled={isCreatingWorkspace}
							onCreateWorkspaceFromRepository={(repository) => {
								onCreateWorkspaceFromProject?.({
									projectId: repository.projectId,
									workspaceRoot: repository.rootPath,
									baseBranch: repository.baseBranch,
									label: repositoryDisplayName(repository),
								});
							}}
							onCreateWorkspace={onCreateWorkspace}
							onCloneWorkspace={onCloneWorkspace}
						/>
					</>
				) : null}

				<div className="scrollbar-stable min-h-0 w-full flex-1 overflow-y-auto px-1 [scrollbar-width:thin]">
					{workspaces.length > 0 ? (
						<div className="flex flex-col items-center gap-1">
							{workspaces.map((workspace) => {
								const label = workspaceRailDisplayTitle(workspace);
								const initials = initialsFromWorkspaceLabel(workspace.name || label);
								const selected = workspace.id === selectedWorkspaceId;
								return (
									<Tooltip key={workspace.id}>
										<TooltipTrigger asChild>
											<button
												type="button"
												aria-current={selected ? "location" : undefined}
												aria-label={t("sidebar.openWorkspace", { label })}
												onClick={() => onSelectWorkspace(workspace.id)}
												className={cn(
													"flex size-9 shrink-0 items-center justify-center rounded-lg text-[10px] font-semibold uppercase ring-1 transition-colors",
													selected
														? "workspace-row-selected text-foreground ring-border"
														: "bg-accent/35 text-muted-foreground ring-transparent hover:bg-accent/60 hover:text-foreground",
												)}
											>
												{initials}
											</button>
										</TooltipTrigger>
										<TooltipContent side="right">{label}</TooltipContent>
									</Tooltip>
								);
							})}
						</div>
					) : (
						<div className="flex h-full min-h-full flex-col items-center justify-center gap-2 px-1 text-center">
							<Tooltip>
								<TooltipTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="icon-xs"
									aria-label={t("sidebar.openProject")}
									className="text-muted-foreground hover:text-foreground"
									disabled={isCreatingWorkspace}
									onClick={onCreateWorkspace}
								>
										<FolderPlus className="size-4" strokeWidth={1.9} aria-hidden />
									</Button>
								</TooltipTrigger>
								<TooltipContent side="right">{t("sidebar.openProject")}</TooltipContent>
							</Tooltip>
							<Tooltip>
								<TooltipTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="icon-xs"
									aria-label={t("sidebar.cloneFromUrl")}
									className="text-muted-foreground hover:text-foreground"
									disabled={isCreatingWorkspace}
									onClick={onCloneWorkspace}
								>
										<Plus className="size-4" strokeWidth={2.1} aria-hidden />
									</Button>
								</TooltipTrigger>
								<TooltipContent side="right">{t("sidebar.cloneFromUrl")}</TooltipContent>
							</Tooltip>
						</div>
					)}
				</div>

				<div className="flex shrink-0 flex-col items-center gap-1 pb-2">
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								className="text-muted-foreground hover:text-foreground"
								aria-label={t("usage.title")}
								onClick={onOpenUsage}
							>
								<BarChart3 className="size-4" strokeWidth={1.85} aria-hidden />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="right">{t("usage.title")}</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								className="relative text-muted-foreground hover:text-foreground"
								aria-label="Skills"
								onClick={onOpenSkills}
							>
								<Sparkles className="size-4" strokeWidth={1.85} aria-hidden />
								{skillCount > 0 ? (
									<span className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full border border-sidebar bg-emerald-500 px-1 text-[9px] font-medium leading-none text-white">
										{skillCount}
									</span>
								) : null}
							</Button>
						</TooltipTrigger>
						<TooltipContent side="right">Skills</TooltipContent>
					</Tooltip>
					{onOpenFeedback && (
						<FeedbackButton onClick={onOpenFeedback} collapsed={collapsed} />
					)}
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								className="text-muted-foreground hover:text-foreground"
								aria-label={t("sidebar.openSettings")}
								onClick={onOpenSettings}
							>
								<Settings2 className="size-4" strokeWidth={1.85} aria-hidden />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="right">{t("sidebar.openSettings")}</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								className="text-muted-foreground hover:text-foreground"
								aria-label={t("sidebar.openHelp")}
								onClick={onOpenHelp}
							>
								<CircleQuestionMark className="size-4" strokeWidth={1.85} aria-hidden />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="right">{t("sidebar.openHelp")}</TooltipContent>
					</Tooltip>
					{onInstallUpdate ? (
						<AppUpdateButton
							update={appUpdate}
							installing={isInstallingUpdate}
							collapsed
							onInstallNow={onInstallUpdate}
						/>
					) : null}
				</div>
			</div>
		);
	}

	return (
		<>
			<div className="dcc-project-sidebar flex h-full min-h-0 flex-col overflow-hidden bg-sidebar">
				<div data-slot="window-safe-top" className="flex h-9 shrink-0 items-center px-3">
					<div
						data-tauri-drag-region
						className="flex h-full min-w-0 flex-1 items-center"
					>
						<div
							aria-label="Dev Command Center"
							className="pointer-events-none flex select-none items-center gap-1.5"
						>
							<img src="/dcc-glyph.svg" alt="" className="size-[18px] shrink-0" />
							<span className="text-[11px] font-semibold tracking-[0.14em] text-foreground/80">
								DCC
							</span>
						</div>
					</div>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								aria-label={t("sidebar.collapseSidebar")}
								variant="ghost"
								size="icon-xs"
								onClick={onToggleCollapsed}
								className="text-muted-foreground hover:text-foreground"
							>
								<PanelLeft className="size-4" strokeWidth={1.8} />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">{t("sidebar.collapseSidebar")}</TooltipContent>
					</Tooltip>
				</div>

				{/* Labels are sized on their spans, like `.dcc-project-label`: the
				    unlayered button font reset in color-theme.css wins over text
				    utilities placed on the buttons. */}
				<div className="dcc-sidebar-navigation space-y-1 px-2 pb-3 pt-1">
					<button
						type="button"
						onClick={onNewTask}
						disabled={isCreatingWorkspace}
						aria-current={newTaskActive ? "page" : undefined}
						className={cn(
							"flex h-8 w-full cursor-pointer items-center gap-2.5 rounded-lg px-2.5 text-left text-[12px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50",
							newTaskActive
								? "bg-accent text-foreground shadow-sm"
								: "text-foreground hover:bg-accent/50",
						)}
					>
						<Plus className="size-4" strokeWidth={1.9} />
						<span className="text-[13px] font-medium">{t("sidebar.newWorkspace")}</span>
					</button>
					<button
						type="button"
						onClick={onOpenPullRequests}
						aria-current={pullRequestsActive ? "page" : undefined}
						className={cn(
							"flex h-8 w-full cursor-pointer items-center gap-2.5 rounded-lg px-2.5 text-left text-[12px] font-medium transition-colors",
							pullRequestsActive
								? "bg-accent text-foreground shadow-sm"
								: "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
						)}
					>
						<GitPullRequest className="size-4" strokeWidth={1.9} />
						<span className="text-[13px] font-medium">{t("sidebar.pullRequests")}</span>
					</button>
					{onOpenNotes && (
						<button
							type="button"
							onClick={onOpenNotes}
							className="dcc-notes-nav flex h-8 w-full cursor-pointer items-center gap-2.5 rounded-lg px-2.5 text-left text-[12px] font-medium text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
						>
							<StickyNote className="size-4" strokeWidth={1.8} />
							<span className="text-[13px] font-medium">{t("notes.headingShort")}</span>
							{notesCount > 0 && (
								<span className="ml-auto rounded-full bg-foreground/5 px-1.5 text-[10px] tabular-nums">{notesCount}</span>
							)}
						</button>
					)}
				</div>

				{onOpenAgent && (
					<AgentsSidebarSection
						agents={agents}
						activeAgentId={activeAgentId}
						onOpenAgent={onOpenAgent}
					/>
				)}

				{attentionItems.length > 0 ? (
					<section
						aria-label={t("sidebar.attention.label", { count: attentionItems.length })}
						className="px-2 pb-3"
					>
						<div className="mb-1 flex items-center gap-1.5 px-1">
							<h2 className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/80">
								{t("sidebar.attention.title")}
							</h2>
							<span className="flex h-4 min-w-4 items-center justify-center rounded-full bg-amber-500/12 px-1 text-[9px] font-semibold tabular-nums text-amber-700 dark:text-amber-300">
								{attentionItems.length}
							</span>
						</div>
						<div className="dcc-attention-panel space-y-px rounded-lg border border-border/45 bg-foreground/[0.018] p-0.5">
							{visibleAttentionItems.map(({ workspace, activity, reason, count }) => {
								const title = workspaceRailDisplayTitle(workspace);
								const sourceKey = projectGroupingKey(workspace);
								const memberProjects = [...new Set(workspace.memberProjectNames ?? [])];
								const projectLabel =
									memberProjects.length > 1
										? memberProjects.join(" · ")
										: projectLabelsBySourceKey.get(sourceKey) ??
											memberProjects[0] ??
											workspace.projectId ??
											t("sidebar.unknownProject");
								const repository = repositoriesBySourceKey.get(sourceKey) ?? null;
								const selected = selectedWorkspaceId === workspace.id;
								const reasonLabel = t(`sidebar.attention.reason.${reason}`, {
									count: count ?? 0,
									pr: count ?? "",
								});

								const canSendChecks = reason === "checksFailing" && Boolean(onSendChecksToAgent);

								return (
									<div key={workspace.id} className="group/attention relative">
									<Tooltip delayDuration={450}>
										<TooltipTrigger asChild>
											<button
												type="button"
												data-attention-reason={reason}
												aria-current={selected ? "location" : undefined}
												aria-label={t("sidebar.attention.openItem", {
													label: title,
													reason: reasonLabel,
												})}
												onClick={() => onSelectWorkspace(workspace.id)}
												className={cn(
													"flex h-11 w-full min-w-0 items-center gap-2 rounded-md px-2 text-left transition-colors",
													selected
														? "workspace-row-selected text-foreground"
														: "text-foreground/85 hover:bg-accent/60 hover:text-foreground",
												)}
											>
												{repository ? (
													<ProjectIdentityGlyph
														icon={projectIconValue(repository)}
														color={repository.color}
														seed={repository.rootPath}
														size="sm"
														className="size-[18px]"
													/>
												) : workspaceProviderIds[workspace.id] ? (
													<ProviderIcon
														provider={workspaceProviderIds[workspace.id]}
														className="size-3.5 shrink-0"
													/>
												) : null}
												<span className="flex min-w-0 flex-1 flex-col">
													<span className="truncate text-[12px] font-medium leading-4">
														{title}
													</span>
													<span className="flex min-w-0 items-center gap-1 text-[10px] leading-4">
														<span
															aria-hidden
															className={cn(
																"size-[6px] shrink-0 rounded-full",
																ATTENTION_DOT_CLASS[reason],
															)}
														/>
														<span
															className={cn(
																"shrink-0 font-medium",
																ATTENTION_TEXT_CLASS[reason],
															)}
														>
															{reasonLabel}
														</span>
														<span aria-hidden className="text-muted-foreground/50">
															·
														</span>
														<span className="truncate text-muted-foreground">
															{projectLabel}
														</span>
													</span>
												</span>
												{activity ? (
													<span className="mt-1.5 shrink-0 self-start whitespace-nowrap text-[10px] font-medium leading-4 text-muted-foreground">
														<WorkspaceActivityTime activity={activity} bare />
													</span>
												) : null}
											</button>
										</TooltipTrigger>
										<TooltipContent side="right">
											{t("sidebar.attention.tooltip", {
												title,
												project: projectLabel,
												reason: reasonLabel,
											})}
										</TooltipContent>
									</Tooltip>
									{canSendChecks ? (
										<button
											type="button"
											aria-label={t("sidebar.attention.sendChecksLabel", { label: title })}
											title={t("sidebar.attention.sendChecks")}
											onClick={() => onSendChecksToAgent?.(workspace.id)}
											className="absolute right-1.5 top-1/2 flex size-6 -translate-y-1/2 items-center justify-center rounded-md bg-background text-muted-foreground opacity-0 shadow-sm ring-1 ring-border transition-opacity hover:text-foreground focus-visible:opacity-100 group-hover/attention:opacity-100"
										>
											<Bot className="size-3.5" aria-hidden />
										</button>
									) : null}
									</div>
								);
							})}
							{hiddenAttentionItemCount > 0 ? (
								<button
									type="button"
									onClick={() => setShowAllAttentionItems(true)}
									className="flex h-7 w-full items-center justify-center rounded-md text-[10.5px] font-medium text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
								>
									{t("sidebar.attention.showMore", { count: hiddenAttentionItemCount })}
								</button>
							) : showAllAttentionItems && attentionItems.length > ATTENTION_PREVIEW_LIMIT ? (
								<button
									type="button"
									onClick={() => setShowAllAttentionItems(false)}
									className="flex h-7 w-full items-center justify-center rounded-md text-[10.5px] font-medium text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
								>
									{t("sidebar.attention.showFewer")}
								</button>
							) : null}
						</div>
					</section>
				) : null}

				<div className="flex items-center justify-between px-3">
					<div className="flex min-w-0 items-center gap-2">
						<h2 className="text-[14px] font-medium tracking-[-0.01em] text-muted-foreground">
							{t("sidebar.title")}
						</h2>
						{runningActivities.length > 0 ? (
							<span
								className="inline-flex items-center gap-1 text-[10.5px] font-medium tabular-nums text-emerald-700 dark:text-emerald-300/90"
								title={t("sidebar.runningTasks", { count: runningActivities.length })}
							>
								<span aria-hidden className="size-[6px] animate-pulse rounded-full bg-emerald-500" />
								{t("sidebar.runningCount", { count: runningActivities.length })}
							</span>
						) : null}
					</div>
					<div className="flex items-center gap-1 text-muted-foreground">
						<DropdownMenu>
							<DropdownMenuTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="icon-xs"
									aria-label={t("sidebar.openProjectMenu")}
									className="text-muted-foreground hover:text-foreground"
									disabled={isCreatingWorkspace}
								>
									<FolderPlus className="size-4" strokeWidth={1.9} aria-hidden />
								</Button>
							</DropdownMenuTrigger>
							<DropdownMenuContent align="end" sideOffset={6} className="min-w-44">
								<DropdownMenuItem
									className="gap-2 text-[13px]"
									onSelect={(event) => {
										event.preventDefault();
										onCreateWorkspace();
									}}
								>
									{t("sidebar.openProject")}
								</DropdownMenuItem>
								<DropdownMenuItem
									className="gap-2 text-[13px]"
									onSelect={(event) => {
										event.preventDefault();
										onCloneWorkspace();
									}}
								>
									{t("sidebar.cloneFromUrl")}
								</DropdownMenuItem>
							</DropdownMenuContent>
						</DropdownMenu>
						<WorkspaceRepoPicker
							repositories={repositories}
							isDisabled={isCreatingWorkspace}
							onCreateWorkspaceFromRepository={(repository) => {
								onCreateWorkspaceFromProject?.({
									projectId: repository.projectId,
									workspaceRoot: repository.rootPath,
									baseBranch: repository.baseBranch,
									label: repositoryDisplayName(repository),
								});
							}}
							onCreateWorkspace={onCreateWorkspace}
							onCloneWorkspace={onCloneWorkspace}
						/>
					</div>
				</div>

				<div
					ref={scrollContainerRef}
					data-slot="workspace-groups-scroll"
					onPointerEnter={() =>
						setFrozenProjectOrder(
							railGroups.filter((group) => !group.pinnedAt).map((group) => group.id),
						)
					}
					onPointerLeave={() => setFrozenProjectOrder(null)}
					className="scrollbar-stable min-h-0 flex-1 overflow-x-hidden overflow-y-auto [scrollbar-width:thin]"
				>
					{activeGroups.length === 0 &&
					waitingRows.length === 0 &&
					completedRows.length === 0 ? (
						<div className="flex h-full min-h-full flex-col items-center justify-center px-4 py-8 text-center">
							<div className="mb-3 flex size-11 items-center justify-center rounded-full border border-border/70 bg-muted/20 text-muted-foreground">
								<FolderPlus className="size-5" strokeWidth={1.9} aria-hidden />
							</div>
							<h3 className="text-[15px] font-medium tracking-[-0.01em] text-foreground">
								{t("sidebar.noWorkspacesYet")}
							</h3>
							<p className="mt-2 max-w-[18rem] text-[13px] leading-6 text-muted-foreground">
								{t("sidebar.noWorkspacesHint")}
							</p>
							<div className="mt-5 flex flex-wrap items-center justify-center gap-2">
								<Button
									type="button"
									size="sm"
									className="gap-1.5"
									disabled={isCreatingWorkspace}
									onClick={onCreateWorkspace}
								>
									<FolderPlus className="size-3.5" strokeWidth={2} aria-hidden />
									{t("sidebar.openProject")}
								</Button>
								<Button
									type="button"
									size="sm"
									variant="outline"
									className="gap-1.5"
									disabled={isCreatingWorkspace}
									onClick={onCloneWorkspace}
								>
									<Plus className="size-3.5" strokeWidth={2} aria-hidden />
									{t("sidebar.cloneFromUrl")}
								</Button>
							</div>
						</div>
					) : (
						<div
							style={{
								height: `${virtualizer.getTotalSize()}px`,
								width: "100%",
								position: "relative",
							}}
						>
							{virtualizer.getVirtualItems().map((vItem) => (
								<div
									key={vItem.key}
									style={{
										position: "absolute",
										top: 0,
										left: 0,
										width: "100%",
										height: `${vItem.size}px`,
										transform: `translateY(${vItem.start}px)`,
									}}
								>
									{renderItem(flatItems[vItem.index]!)}
								</div>
							))}
						</div>
					)}
				</div>

				<div className="dcc-sidebar-tools flex shrink-0 items-center justify-start gap-1 px-1 pb-3 pt-2">
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								className="text-muted-foreground hover:text-foreground"
								aria-label={t("usage.title")}
								onClick={onOpenUsage}
							>
								<BarChart3 className="size-4" strokeWidth={1.85} aria-hidden />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">{t("usage.title")}</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								className="relative text-muted-foreground hover:text-foreground"
								aria-label="Skills"
								onClick={onOpenSkills}
							>
								<Sparkles className="size-4" strokeWidth={1.85} aria-hidden />
								{skillCount > 0 ? (
									<span className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full border border-sidebar bg-emerald-500 px-1 text-[9px] font-medium leading-none text-white">
										{skillCount}
									</span>
								) : null}
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">Skills</TooltipContent>
					</Tooltip>
					{onOpenFeedback && (
						<FeedbackButton onClick={onOpenFeedback} collapsed={collapsed} />
					)}
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								className="text-muted-foreground hover:text-foreground"
								aria-label={t("sidebar.openSettings")}
								onClick={onOpenSettings}
							>
								<Settings2 className="size-4" strokeWidth={1.85} aria-hidden />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">{t("sidebar.openSettings")}</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								className="text-muted-foreground hover:text-foreground"
								aria-label={t("sidebar.openHelp")}
								onClick={onOpenHelp}
							>
								<CircleQuestionMark className="size-4" strokeWidth={1.85} aria-hidden />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">{t("sidebar.openHelp")}</TooltipContent>
					</Tooltip>
					{onInstallUpdate ? (
						<AppUpdateButton
							update={appUpdate}
							installing={isInstallingUpdate}
							className="ml-auto"
							onInstallNow={onInstallUpdate}
						/>
					) : null}
				</div>
			</div>

			<ProjectEditDialog
				repository={projectEditTarget}
				open={projectEditTarget !== null}
				onOpenChange={(open) => {
					if (!open) setProjectEditTarget(null);
				}}
				onSave={async (input) => {
					if (!onUpdateProjectIdentity) return;
					await onUpdateProjectIdentity(input);
				}}
			/>

			<Dialog
				open={projectRemovalTarget !== null}
				onOpenChange={(open) => {
					if (!open && !isRemovingProject) {
						setProjectRemovalTarget(null);
					}
				}}
			>
				<DialogContent showCloseButton={!isRemovingProject}>
					<DialogHeader>
						<DialogTitle>
							{t("sidebar.removeProjectTitle", {
								label: projectRemovalTarget?.label ?? "",
							})}
						</DialogTitle>
						<DialogDescription>
							{t("sidebar.removeProjectDescription", {
								count: projectRemovalTarget?.workspaceCount ?? 0,
							})}
						</DialogDescription>
					</DialogHeader>
					{projectRemovalTarget?.rootPath ? (
						<div className="rounded-lg border border-border/70 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
							<span className="font-medium text-foreground">
								{t("sidebar.removeProjectPathLabel")}
							</span>
							<span className="ml-2 break-all font-mono">
								{projectRemovalTarget.rootPath}
							</span>
						</div>
					) : null}
					<p className="text-xs leading-5 text-muted-foreground">
						{t("sidebar.removeProjectWarning")}
					</p>
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							disabled={isRemovingProject}
							onClick={() => setProjectRemovalTarget(null)}
						>
							{t("sidebar.cancel")}
						</Button>
						<Button
							type="button"
							variant="destructive"
							disabled={isRemovingProject || !projectRemovalTarget || !onDeleteProject}
							onClick={() => {
								void handleConfirmProjectRemoval();
							}}
						>
							{isRemovingProject ? (
								<>
									<Loader2 className="size-3.5 animate-spin" aria-hidden />
									{t("sidebar.removeProjectConfirming")}
								</>
							) : (
								t("sidebar.removeProjectConfirm")
							)}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>

			{onDeleteWorkspace ? (
				<CompletedCleanupDialog
					open={isCleanupOpen}
					onOpenChange={setIsCleanupOpen}
					workspaces={completedRows}
					projectLabelOf={(workspace) => {
						const repository = workspace.rootPath
							? repositoriesBySourceKey.get(workspace.rootPath.trim())
							: undefined;
						return repository ? repositoryDisplayName(repository) : undefined;
					}}
					onDeleteWorkspace={onDeleteWorkspace}
					onCleaned={dismissCleanupAlert}
				/>
			) : null}

			<Dialog
				open={workspaceDeletionTarget !== null}
				onOpenChange={(open) => {
					if (!open && !isDeletingWorkspace) {
						setWorkspaceDeletionTarget(null);
					}
				}}
			>
				<DialogContent
					showCloseButton={!isDeletingWorkspace}
					className="min-w-0 overflow-hidden sm:max-w-md"
				>
					<DialogHeader className="min-w-0 pr-7">
						<DialogTitle className="min-w-0 text-pretty leading-snug [overflow-wrap:anywhere]">
							{t("sidebar.deleteWorkspaceTitle", {
								label: workspaceDeletionTarget
									? workspaceRailDisplayTitle(workspaceDeletionTarget)
									: "",
							})}
						</DialogTitle>
						<DialogDescription className="min-w-0 break-words leading-5">
							{t("sidebar.deleteWorkspaceDescription")}
						</DialogDescription>
					</DialogHeader>
					{workspaceDeletionTarget &&
					completedDiskUsage.status === "loading" ? (
						<div className="flex items-center gap-2 rounded-lg border border-border/70 bg-muted/20 px-3 py-2.5 text-sm text-muted-foreground">
							<Loader2 className="size-3.5 animate-spin" aria-hidden />
							{t("sidebar.calculatingWorkspaceDiskUsage")}
						</div>
					) : workspaceDeletionBytes !== null ? (
						<p className="rounded-lg border border-border/70 bg-muted/20 px-3 py-2.5 text-sm text-muted-foreground">
							{t("sidebar.workspaceDiskSpaceFreed", {
								size: formatDiskBytes(
									workspaceDeletionBytes,
									i18n.resolvedLanguage,
								),
							})}
						</p>
					) : null}
					{workspaceDeletionTarget?.remoteDeletionTargets?.length ? (
						<label className="flex min-w-0 max-w-full cursor-pointer items-start gap-2.5 overflow-hidden rounded-lg border border-border/70 bg-muted/20 px-3 py-2.5 text-sm">
							<input
								type="checkbox"
								checked={deleteRemoteBranch}
								disabled={isDeletingWorkspace}
								onChange={(event) => setDeleteRemoteBranch(event.target.checked)}
								className="mt-0.5 size-4 accent-primary"
							/>
							<span className="min-w-0 flex-1 overflow-hidden">
								<span className="block font-medium text-foreground">
									{workspaceDeletionTarget.bundleId
										? t("sidebar.deleteRemoteBranches")
										: t("sidebar.deleteRemoteBranch")}
								</span>
								<span
									className="mt-0.5 block max-w-full truncate font-mono text-xs text-muted-foreground"
									title={[
										...new Set(
											workspaceDeletionTarget.remoteDeletionTargets.map(
												(target) => `${target.remote}/${target.branch}`,
											),
										),
									].join(", ")}
								>
									{[
										...new Set(
											workspaceDeletionTarget.remoteDeletionTargets.map(
												(target) => `${target.remote}/${target.branch}`,
											),
										),
									].join(", ")}
								</span>
							</span>
						</label>
					) : (
						<p className="rounded-lg border border-border/70 bg-muted/20 px-3 py-2.5 text-sm text-muted-foreground">
							{t("sidebar.noRemoteBranchToDelete")}
						</p>
					)}
					<DialogFooter className="min-w-0">
						<Button
							type="button"
							variant="outline"
							disabled={isDeletingWorkspace}
							onClick={() => setWorkspaceDeletionTarget(null)}
						>
							{t("sidebar.cancel")}
						</Button>
						<Button
							type="button"
							variant="destructive"
							disabled={
								isDeletingWorkspace ||
								!workspaceDeletionTarget ||
								!onDeleteWorkspace
							}
							onClick={() => {
								void handleConfirmWorkspaceDeletion();
							}}
						>
							{isDeletingWorkspace ? (
								<>
									<Loader2 className="size-3.5 animate-spin" aria-hidden />
									{t("sidebar.deletingWorkspace")}
								</>
							) : (
								t("sidebar.deleteWorkspaceConfirm")
							)}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</>
	);
});
