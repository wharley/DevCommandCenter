import { useVirtualizer } from "@tanstack/react-virtual";
import {
	Archive,
	ChevronRight,
	FolderPlus,
	MoreHorizontal,
	PanelLeft,
	PanelRight,
	Plus,
	Settings2,
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
import { toast } from "sonner";
import { Badge } from "../../components/ui/badge";
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
import { Tooltip, TooltipContent, TooltipTrigger } from "../../components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { WorkspaceSummary } from "./types";
import {
	createInitialRailSectionState,
	readStoredRailSectionState,
	writeStoredRailSectionState,
} from "./workspace-rail-open-state";
import {
	projectGroupingKey,
	projectWorkspaceRailGroups,
	projectWorkspaceRepositories,
	type DccWorkspaceRepository,
} from "./workspace-rail-projection";
import {
	ARCHIVED_SECTION_ID,
	findSelectedRailSectionId,
	humanizeWorkspaceBranchLabel,
	initialsFromWorkspaceLabel,
	ProjectGroupGlyph,
} from "./workspace-rail-shared";
import { WorkspaceRailRowItem } from "./workspace-rail-row";
import { useWorkspaceAgentStates } from "./use-workspace-agent-states";

type VirtualItem =
	| {
			kind: "group-header";
			groupId: string;
			label: string;
			sourceKey?: string;
			rowCount: number;
			canCollapse: boolean;
			headerVariant: "project" | "archived";
	  }
	| { kind: "row"; groupId: string; workspace: WorkspaceSummary }
	| { kind: "group-gap"; size: number }
	| { kind: "bottom-padding" };

const HEADER_HEIGHT = 34;
const ROW_HEIGHT = 32;
const GROUP_GAP = 8;
const EMPTY_GROUP_GAP = 8;
const BOTTOM_PADDING = 8;

function getGroupGapSize(previousHasRows: boolean, nextHasRows: boolean) {
	return previousHasRows && nextHasRows ? GROUP_GAP : EMPTY_GROUP_GAP;
}

function TrafficLightSpacer({ width }: { width: number }) {
	return <div aria-hidden className="shrink-0" style={{ width }} />;
}

function WorkspaceRepoPicker({
	repositories,
	isDisabled = false,
	onCreateWorkspaceFromRepository,
	onCreateWorkspace,
	onCloneWorkspace,
}: {
	repositories: DccWorkspaceRepository[];
	isDisabled?: boolean;
	onCreateWorkspaceFromRepository: (repository: DccWorkspaceRepository) => void;
	onCreateWorkspace: () => void;
	onCloneWorkspace: () => void;
}) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);

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
						{repositories.map((repository) => (
							<CommandItem
								key={repository.sourceKey}
								value={`${repository.label} ${repository.projectId} ${repository.branch} ${repository.workspaceRoot}`}
								onSelect={() => {
									setOpen(false);
									onCreateWorkspaceFromRepository(repository);
								}}
							>
								<strong className="truncate">{repository.label}</strong>
								<span className="truncate text-[var(--dcc-text-muted)]">
									{repository.branch}
								</span>
								<span className="truncate text-[10px] text-muted-foreground">
									{repository.workspaceRoot}
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
						>
							{t("sidebar.openRepoPickerAction")}
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
	onSelectWorkspace: (workspaceId: string) => void;
	onCreateWorkspace: () => void;
	onCloneWorkspace: () => void;
	onCreateWorkspaceFromProject?: (input: {
		projectId: string;
		workspaceRoot: string;
		label: string;
	}) => void;
	onOpenSettings: () => void;
	onToggleCollapsed: () => void;
	onArchiveWorkspace?: (workspaceId: string) => void;
	onRestoreWorkspace?: (workspaceId: string) => void;
	onDeleteWorkspace?: (workspaceId: string) => void;
	onDeleteProject?: (workspaceIds: string[]) => Promise<void> | void;
	selectedWorkspaceId: string | null;
	workspaces: WorkspaceSummary[];
};

type ProjectRemovalTarget = {
	label: string;
	rootPath: string | null;
	workspaceCount: number;
	workspaceIds: string[];
};

export const WorkspacesSidebar = memo(function WorkspacesSidebar({
	collapsed,
	isCreatingWorkspace = false,
	onSelectWorkspace,
	onCreateWorkspace,
	onCloneWorkspace,
	onCreateWorkspaceFromProject,
	onOpenSettings,
	onToggleCollapsed,
	onArchiveWorkspace,
	onRestoreWorkspace,
	onDeleteWorkspace,
	onDeleteProject,
	selectedWorkspaceId,
	workspaces,
}: WorkspacesSidebarProps) {
	const { t } = useTranslation("common");
	const workspaceAgentStates = useWorkspaceAgentStates(workspaces);
	const scrollContainerRef = useRef<HTMLDivElement>(null);
	const { activeGroups, archivedRows } = useMemo(
		() => projectWorkspaceRailGroups(workspaces),
		[workspaces],
	);
	const repositories = useMemo(() => projectWorkspaceRepositories(workspaces), [workspaces]);
	const repositoriesBySourceKey = useMemo(
		() => new Map(repositories.map((repository) => [repository.sourceKey, repository])),
		[repositories],
	);
	const [projectRemovalTarget, setProjectRemovalTarget] = useState<ProjectRemovalTarget | null>(
		null,
	);
	const [isRemovingProject, setIsRemovingProject] = useState(false);

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

			const archivedValue = current[ARCHIVED_SECTION_ID] ?? false;
			next[ARCHIVED_SECTION_ID] = archivedValue;
			if (current[ARCHIVED_SECTION_ID] !== archivedValue) {
				changed = true;
			}

			if (Object.keys(current).length !== Object.keys(next).length) {
				changed = true;
			}

			return changed ? next : current;
		});
	}, [activeGroups, archivedRows]);

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
			archivedRows,
		);

		if (!selectedSectionId) {
			return;
		}

		lastAutoExpandedIdRef.current = selectedWorkspaceId;
		setSectionOpenState((current) =>
			current[selectedSectionId] ? current : { ...current, [selectedSectionId]: true },
		);
	}, [activeGroups, archivedRows, selectedWorkspaceId]);

	const flatItems = useMemo(() => {
		const items: VirtualItem[] = [];
		const visibleGroups = activeGroups.filter((g) => g.rows.length > 0);

		for (let gi = 0; gi < visibleGroups.length; gi++) {
			const group = visibleGroups[gi]!;
			if (gi > 0) {
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

		const previousGroup = visibleGroups.at(-1);
		items.push({
			kind: "group-gap",
			size: getGroupGapSize(
				(previousGroup?.rows.length ?? 0) > 0,
				archivedRows.length > 0,
			),
		});
		items.push({
			kind: "group-header",
			groupId: ARCHIVED_SECTION_ID,
			label: t("sidebar.archived"),
			rowCount: archivedRows.length,
			canCollapse: archivedRows.length > 0,
			headerVariant: "archived",
		});

		if (sectionOpenState[ARCHIVED_SECTION_ID] && archivedRows.length > 0) {
			for (const row of archivedRows) {
				items.push({
					kind: "row",
					groupId: ARCHIVED_SECTION_ID,
					workspace: row,
				});
			}
		}

		items.push({ kind: "bottom-padding" });
		return items;
	}, [activeGroups, archivedRows, sectionOpenState, t]);

	const virtualizer = useVirtualizer({
		count: flatItems.length,
		getScrollElement: () => scrollContainerRef.current,
		estimateSize: (index) => {
			const item = flatItems[index]!;
			switch (item.kind) {
				case "group-header":
					return HEADER_HEIGHT;
				case "row":
					return ROW_HEIGHT;
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
			if (matchingWorkspaces.length === 0) {
				return;
			}

			const rootPath =
				matchingWorkspaces.find((workspace) => workspace.rootPath?.trim())?.rootPath?.trim() ??
				matchingWorkspaces.find((workspace) => workspace.worktreePath?.trim())?.worktreePath?.trim() ??
				null;

			setProjectRemovalTarget({
				label,
				rootPath,
				workspaceCount: matchingWorkspaces.length,
				workspaceIds: matchingWorkspaces.map((workspace) => workspace.id),
			});
		},
		[workspaces],
	);

	const handleConfirmProjectRemoval = useCallback(async () => {
		if (!projectRemovalTarget || !onDeleteProject) {
			return;
		}

		setIsRemovingProject(true);
		try {
			await onDeleteProject(projectRemovalTarget.workspaceIds);
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

	const renderItem = useCallback(
		(item: VirtualItem) => {
			if (item.kind === "group-gap" || item.kind === "bottom-padding") {
				return null;
			}

			if (item.kind === "group-header") {
				const isOpen =
					item.groupId === ARCHIVED_SECTION_ID
						? (sectionOpenState[item.groupId] ?? false)
						: (sectionOpenState[item.groupId] ?? true);
				const isEmptyGroup = item.rowCount === 0;
				const repository =
					item.headerVariant === "project" && item.sourceKey
						? repositoriesBySourceKey.get(item.sourceKey) ?? null
						: null;
				const canCreateProjectWorkspace =
					item.headerVariant === "project" &&
					repository !== null &&
					Boolean(onCreateWorkspaceFromProject) &&
					!isCreatingWorkspace;
				const canRemoveProject =
					item.headerVariant === "project" &&
					Boolean(item.sourceKey) &&
					Boolean(onDeleteProject) &&
					!isRemovingProject;

				return (
					<div
						className={cn(
							"group/dccRailHeader flex items-center gap-1 rounded-lg pr-1 hover:bg-accent/60",
						)}
						data-empty-group={isEmptyGroup ? "true" : "false"}
					>
						<button
							type="button"
							className="flex min-w-0 flex-1 cursor-pointer select-none items-center justify-between rounded-lg px-2 py-1 text-[13px] font-semibold tracking-[-0.01em] text-foreground"
							disabled={!item.canCollapse}
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
								{item.headerVariant === "archived" ? (
									<Archive
										className="size-[14px] shrink-0 text-[var(--workspace-sidebar-status-backlog)]"
										strokeWidth={1.9}
										aria-hidden
									/>
								) : (
									<ProjectGroupGlyph />
								)}
								<span className="truncate">{item.label}</span>
							</span>

							{item.rowCount > 0 ? (
								<Badge
									variant="secondary"
									className="h-4 min-w-[16px] justify-center rounded-full px-1 text-[9.5px]"
								>
									{item.rowCount}
								</Badge>
							) : null}
						</button>

						{canCreateProjectWorkspace ? (
							<Tooltip>
								<TooltipTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="icon-xs"
										aria-label={t("sidebar.createWorkspaceFromProject", {
											label: item.label,
										})}
										className="shrink-0 text-muted-foreground opacity-0 transition-opacity hover:text-foreground group-hover/dccRailHeader:opacity-100 group-focus-within/dccRailHeader:opacity-100"
										onClick={(event) => {
											event.stopPropagation();
											onCreateWorkspaceFromProject?.({
												projectId: repository.projectId,
												workspaceRoot: repository.workspaceRoot,
												label: repository.label,
											});
										}}
									>
										<Plus className="size-4" strokeWidth={2.1} aria-hidden />
									</Button>
								</TooltipTrigger>
								<TooltipContent side="top">
									{t("sidebar.newWorkspace")}
								</TooltipContent>
							</Tooltip>
						) : null}

						{canRemoveProject ? (
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
									{repository && onCreateWorkspaceFromProject ? (
										<DropdownMenuItem
											className="gap-2 text-[13px]"
											onSelect={(event) => {
												event.preventDefault();
												onCreateWorkspaceFromProject({
													projectId: repository.projectId,
													workspaceRoot: repository.workspaceRoot,
													label: repository.label,
												});
											}}
										>
											<Plus className="size-3.5" strokeWidth={2} aria-hidden />
											{t("sidebar.newWorkspace")}
										</DropdownMenuItem>
									) : null}
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
								</DropdownMenuContent>
							</DropdownMenu>
						) : null}
					</div>
				);
			}

			return (
				<WorkspaceRailRowItem
					workspace={item.workspace}
					selected={selectedWorkspaceId === item.workspace.id}
					agentState={workspaceAgentStates[item.workspace.id] ?? null}
					onSelect={onSelectWorkspace}
					onArchiveWorkspace={onArchiveWorkspace}
					onRestoreWorkspace={onRestoreWorkspace}
					onDeleteWorkspace={onDeleteWorkspace}
				/>
			);
		},
		[
			isCreatingWorkspace,
			isRemovingProject,
			onArchiveWorkspace,
			onCreateWorkspaceFromProject,
			onDeleteProject,
			onDeleteWorkspace,
			onRestoreWorkspace,
			onSelectWorkspace,
			openProjectRemovalDialog,
			repositoriesBySourceKey,
			sectionOpenState,
			selectedWorkspaceId,
			t,
			toggleSection,
			workspaceAgentStates,
		],
	);

	if (collapsed) {
		return (
			<div className="flex h-full min-h-0 flex-col items-center gap-2 overflow-hidden py-2">
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
									workspaceRoot: repository.workspaceRoot,
									label: repository.label,
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
								const label = workspace.branch
									? humanizeWorkspaceBranchLabel(workspace.branch)
									: workspace.name;
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
								aria-label={t("sidebar.openSettings")}
								onClick={onOpenSettings}
							>
								<Settings2 className="size-4" strokeWidth={1.85} aria-hidden />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="right">{t("sidebar.openSettings")}</TooltipContent>
					</Tooltip>
				</div>
			</div>
		);
	}

	return (
		<>
			<div className="flex h-full min-h-0 flex-col overflow-hidden bg-sidebar">
				<div data-slot="window-safe-top" className="flex h-9 shrink-0 items-center pr-3">
					<TrafficLightSpacer width={94} />
					<div data-tauri-drag-region className="h-full flex-1" />
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

				<div className="flex items-center justify-between px-3">
					<h2 className="text-[14px] font-medium tracking-[-0.01em] text-muted-foreground">
						{t("sidebar.title")}
					</h2>
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
									workspaceRoot: repository.workspaceRoot,
									label: repository.label,
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
					className="min-h-0 flex-1 overflow-hidden"
				>
					{activeGroups.length === 0 && archivedRows.length === 0 ? (
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

				<div className="flex shrink-0 items-center justify-start gap-1 px-3 pb-3 pt-1">
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="gap-1.5 text-muted-foreground hover:text-foreground"
								aria-label={t("sidebar.openSettings")}
								onClick={onOpenSettings}
							>
								<Settings2 className="size-4" strokeWidth={1.85} aria-hidden />
								<span className="text-xs font-medium">{t("sidebar.settingsShort")}</span>
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">{t("sidebar.openSettings")}</TooltipContent>
					</Tooltip>
				</div>
			</div>

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
							{t("sidebar.removeProjectConfirm")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</>
	);
});
