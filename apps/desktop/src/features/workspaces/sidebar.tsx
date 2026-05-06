import { useVirtualizer } from "@tanstack/react-virtual";
import {
	Archive,
	ChevronRight,
	FolderPlus,
	PanelLeft,
	PanelRight,
	Plus,
	Settings2,
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
import { projectWorkspaceRailGroups } from "./workspace-rail-projection";
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
	workspaces,
	onCreateWorkspace,
	onCloneWorkspace,
}: {
	workspaces: WorkspaceSummary[];
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
						{workspaces.map((workspace) => (
							<CommandItem
								key={workspace.id}
								value={`${workspace.name} ${workspace.branch} ${workspace.rootPath ?? ""}`}
								onSelect={() => {
									setOpen(false);
								}}
							>
								<strong className="truncate">{workspace.name}</strong>
								<span className="truncate text-[var(--dcc-text-muted)]">
									{workspace.branch}
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
	onOpenSettings: () => void;
	onToggleCollapsed: () => void;
	onArchiveWorkspace?: (workspaceId: string) => void;
	onRestoreWorkspace?: (workspaceId: string) => void;
	onDeleteWorkspace?: (workspaceId: string) => void;
	selectedWorkspaceId: string | null;
	workspaces: WorkspaceSummary[];
};

export const WorkspacesSidebar = memo(function WorkspacesSidebar({
	collapsed,
	isCreatingWorkspace = false,
	onSelectWorkspace,
	onCreateWorkspace,
	onCloneWorkspace,
	onOpenSettings,
	onToggleCollapsed,
	onArchiveWorkspace,
	onRestoreWorkspace,
	onDeleteWorkspace,
	selectedWorkspaceId,
	workspaces,
}: WorkspacesSidebarProps) {
	const { t } = useTranslation("common");
	const workspaceAgentStates = useWorkspaceAgentStates();
	const scrollContainerRef = useRef<HTMLDivElement>(null);
	const { activeGroups, archivedRows } = useMemo(
		() => projectWorkspaceRailGroups(workspaces),
		[workspaces],
	);

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

				return (
					<button
						type="button"
						className={cn(
							"flex w-full cursor-pointer select-none items-center justify-between rounded-lg px-2 py-1 text-[13px] font-semibold tracking-[-0.01em] text-foreground hover:bg-accent/60",
						)}
						data-empty-group={isEmptyGroup ? "true" : "false"}
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
		[sectionOpenState, selectedWorkspaceId, workspaceAgentStates, toggleSection, onSelectWorkspace, onArchiveWorkspace, onRestoreWorkspace, onDeleteWorkspace],
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
						workspaces={workspaces}
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
	);
});
