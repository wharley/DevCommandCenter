/**
 * Git changes list — staged / unstaged groups, list or tree view, per-file +/−,
 * extension icons and NumberTicker for diff stats.
 */

import { useQueryClient } from "@tanstack/react-query";
import { getMaterialFileIcon, getMaterialFolderIcon } from "file-extension-icon-js";
import {
	ChevronRight,
	CloudIcon,
	LaptopIcon,
	List as ListIcon,
	ListTree,
	LoaderCircleIcon,
	MinusIcon,
	PlusIcon,
	Undo2Icon,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { AnimatedShinyText } from "@/components/ui/animated-shiny-text";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { NumberTicker } from "@/components/ui/number-ticker";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { WorkspaceGitChangeEntry } from "@dcc/contracts";
import {
	workspaceGitDiscardFile,
	workspaceGitStageAll,
	workspaceGitStageFile,
	workspaceGitUnstageFile,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import { CodeRabbitReviewSection } from "./coderabbit-review-section";
import { useWorkspaceGitStatus, WORKSPACE_GIT_STATUS_QUERY_KEY } from "./use-workspace-git-status";
import { useWorkspaceGitBranchDiff, WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY } from "./use-workspace-git-branch-diff";
import type { WorkspaceGitPreviewSelection } from "./workspace-git-file-preview";

export { WORKSPACE_GIT_STATUS_QUERY_KEY, WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY };

const STATUS_BADGE_CLASS: Record<string, string> = {
	M: "text-yellow-600 dark:text-yellow-500",
	A: "text-green-600 dark:text-green-500",
	D: "text-red-600 dark:text-red-500",
	U: "text-orange-600 dark:text-orange-500",
	R: "text-violet-600 dark:text-violet-500",
	C: "text-violet-600 dark:text-violet-500",
	"?": "text-blue-600 dark:text-blue-500",
};

function statusClass(status: string): string {
	return STATUS_BADGE_CLASS[status] ?? "text-muted-foreground";
}

/**
 * Floating action toolbar anchored to the row's right edge. Pulled out of the
 * flex flow so a long file path can never squeeze the add/discard/unstage
 * buttons off-screen — they always sit on top, at the right, on hover.
 */
const ROW_ACTIONS_CLASS =
	"absolute right-1 top-1/2 z-10 hidden -translate-y-1/2 items-center gap-0.5 rounded-md bg-background/95 px-0.5 py-px shadow-sm ring-1 ring-border/60 backdrop-blur-sm group-hover/row:flex";

function dirname(path: string): string {
	const i = path.lastIndexOf("/");
	return i <= 0 ? "" : path.slice(0, i);
}

function RowIconButton({
	"aria-label": ariaLabel,
	onClick,
	disabled,
	children,
	className,
}: {
	"aria-label": string;
	onClick: () => void;
	disabled?: boolean;
	children: React.ReactNode;
	className?: string;
}) {
	return (
		<Button
			type="button"
			variant="ghost"
			size="icon-xs"
			aria-label={ariaLabel}
			disabled={disabled}
			onClick={(event) => {
				event.stopPropagation();
				onClick();
			}}
			onKeyDown={(event) => event.stopPropagation()}
			className={cn(
				"size-4 rounded-sm transition-colors disabled:pointer-events-none disabled:opacity-60",
				className,
			)}
		>
			{children}
		</Button>
	);
}

type TreeNode = {
	name: string;
	path: string;
	children: Map<string, TreeNode>;
	file?: WorkspaceGitChangeEntry;
};

function buildTree(entries: WorkspaceGitChangeEntry[]): TreeNode {
	const root: TreeNode = { name: "", path: "", children: new Map() };

	for (const entry of entries) {
		const parts = entry.path.split("/");
		let current = root;
		for (let index = 0; index < parts.length - 1; index += 1) {
			const part = parts[index]!;
			if (!current.children.has(part)) {
				current.children.set(part, {
					name: part,
					path: parts.slice(0, index + 1).join("/"),
					children: new Map(),
				});
			}
			current = current.children.get(part)!;
		}
		const fileName = parts[parts.length - 1]!;
		current.children.set(fileName, {
			name: fileName,
			path: entry.path,
			children: new Map(),
			file: entry,
		});
	}

	return root;
}

function collectFolderPaths(node: TreeNode): string[] {
	const paths: string[] = [];
	for (const child of node.children.values()) {
		if (child.children.size > 0 && !child.file) {
			paths.push(child.path);
			paths.push(...collectFolderPaths(child));
		}
	}
	return paths;
}

function ChangeRow({
	entry,
	group,
	workspaceRoot,
	gitBusy,
	runGit,
	treeIndentPx = 0,
	fileIconSrc,
	flashingPaths = new Set(),
	selected = false,
	onSelect,
}: {
	entry: WorkspaceGitChangeEntry;
	group: "staged" | "unstaged" | "committed";
	workspaceRoot: string;
	gitBusy: boolean;
	runGit: (fn: () => Promise<void>) => Promise<void>;
	treeIndentPx?: number;
	fileIconSrc?: string;
	flashingPaths?: Set<string>;
	selected?: boolean;
	onSelect?: (selection: WorkspaceGitPreviewSelection) => void;
}) {
	const folder = dirname(entry.path);
	const input = { workspaceRoot, relativePath: entry.path };
	const iconSrc = fileIconSrc ?? getMaterialFileIcon(entry.name);

	return (
		<div
			className={cn(
				"group/row relative flex items-center gap-1.5 py-[1.5px] pl-2 pr-2 text-[11.5px] text-muted-foreground transition-colors hover:bg-accent/60",
				onSelect && "cursor-pointer",
				selected && "bg-muted/60 text-foreground",
			)}
			style={treeIndentPx > 0 ? { paddingLeft: treeIndentPx } : undefined}
			title={entry.absolutePath}
			role={onSelect ? "button" : undefined}
			tabIndex={onSelect ? 0 : undefined}
			onClick={() =>
				onSelect?.({
					group,
					path: entry.path,
					name: entry.name,
					status: entry.status,
					baseBranch: null,
				})
			}
			onKeyDown={(event) => {
				if (!onSelect) return;
				if (event.key === "Enter" || event.key === " ") {
					event.preventDefault();
					onSelect({
						group,
						path: entry.path,
						name: entry.name,
						status: entry.status,
						baseBranch: null,
					});
				}
			}}
		>
			<img src={iconSrc} alt="" className="size-3.5 shrink-0" />
			<span className="min-w-0 max-w-[40%] truncate font-medium text-foreground sm:max-w-[52%]">
				<ShinyFlash active={flashingPaths.has(entry.path)}>{entry.name}</ShinyFlash>
			</span>
			<span
				className={cn(
					"min-w-0 flex-1 truncate text-right text-[10px] text-muted-foreground",
					"group-hover/row:hidden",
				)}
			>
				{folder}
			</span>
			<span
				className={cn(
					"flex shrink-0 items-center gap-1 tabular-nums",
					"group-hover/row:hidden",
				)}
			>
				{entry.insertions > 0 ? (
					<span className="text-[10px] text-emerald-600 dark:text-emerald-400">
						+<NumberTicker value={entry.insertions} className="text-[10px]" />
					</span>
				) : null}
				{entry.deletions > 0 ? (
					<span className="text-[10px] text-destructive">
						−<NumberTicker value={entry.deletions} className="text-[10px]" direction="down" />
					</span>
				) : null}
				<span
					className={cn(
						"inline-flex h-4 min-w-[1rem] items-center justify-center text-[10px] font-semibold",
						statusClass(entry.status),
					)}
				>
					{entry.status}
				</span>
			</span>
			{group === "staged" ? (
				<span className={ROW_ACTIONS_CLASS}>
					<RowIconButton
						aria-label="Unstage file"
						disabled={gitBusy}
						onClick={() =>
							void runGit(async () => {
								await workspaceGitUnstageFile(input);
							})
						}
						className="text-muted-foreground hover:bg-accent/60 hover:text-foreground"
					>
						<MinusIcon className="size-3.5" strokeWidth={2} />
					</RowIconButton>
				</span>
			) : group === "unstaged" ? (
				<span className={ROW_ACTIONS_CLASS}>
					<RowIconButton
						aria-label="Discard file changes"
						disabled={gitBusy}
						onClick={() =>
							void runGit(async () => {
								await workspaceGitDiscardFile(input);
							})
						}
						className="text-muted-foreground hover:bg-accent/60 hover:text-foreground"
					>
						<Undo2Icon className="size-3.5" strokeWidth={2} />
					</RowIconButton>
					<RowIconButton
						aria-label="Stage file"
						disabled={gitBusy}
						onClick={() =>
							void runGit(async () => {
								await workspaceGitStageFile(input);
							})
						}
						className="text-muted-foreground hover:bg-accent/60 hover:text-foreground"
					>
						<PlusIcon className="size-3.5" strokeWidth={2} />
					</RowIconButton>
				</span>
			) : null}
		</div>
	);
}

function ChangesTreeView({
	entries,
	group,
	workspaceRoot,
	gitBusy,
	runGit,
	flashingPaths = new Set(),
	selectedPath = null,
	onSelect,
}: {
	entries: WorkspaceGitChangeEntry[];
	group: "staged" | "unstaged" | "committed";
	workspaceRoot: string;
	gitBusy: boolean;
	runGit: (fn: () => Promise<void>) => Promise<void>;
	flashingPaths?: Set<string>;
	selectedPath?: string | null;
	onSelect?: (selection: WorkspaceGitPreviewSelection) => void;
}) {
	const tree = useMemo(() => buildTree(entries), [entries]);
	const [expanded, setExpanded] = useState<Set<string>>(() => new Set(collectFolderPaths(tree)));

	const toggle = (path: string) => {
		setExpanded((previous) => {
			const next = new Set(previous);
			if (next.has(path)) {
				next.delete(path);
			} else {
				next.add(path);
			}
			return next;
		});
	};

	return (
		<div className="pb-2 pl-1">
			<TreeNodeList
				nodes={tree.children}
				expanded={expanded}
				onToggle={toggle}
				depth={0}
				group={group}
				workspaceRoot={workspaceRoot}
				gitBusy={gitBusy}
				runGit={runGit}
				flashingPaths={flashingPaths}
				selectedPath={selectedPath}
				onSelect={onSelect}
			/>
		</div>
	);
}

function TreeNodeList({
	nodes,
	expanded,
	onToggle,
	depth,
	group,
	workspaceRoot,
	gitBusy,
	runGit,
	flashingPaths = new Set(),
	selectedPath = null,
	onSelect,
}: {
	nodes: Map<string, TreeNode>;
	expanded: Set<string>;
	onToggle: (path: string) => void;
	depth: number;
	group: "staged" | "unstaged" | "committed";
	workspaceRoot: string;
	gitBusy: boolean;
	runGit: (fn: () => Promise<void>) => Promise<void>;
	flashingPaths?: Set<string>;
	selectedPath?: string | null;
	onSelect?: (selection: WorkspaceGitPreviewSelection) => void;
}) {
	const sorted = [...nodes.values()].sort((left, right) => {
		const leftIsFolder = left.children.size > 0 && !left.file;
		const rightIsFolder = right.children.size > 0 && !right.file;
		if (leftIsFolder !== rightIsFolder) {
			return leftIsFolder ? -1 : 1;
		}
		return left.name.localeCompare(right.name);
	});

	const pad = depth * 12 + 8;

	return (
		<>
			{sorted.map((node) => {
				const isFolder = node.children.size > 0 && !node.file;

				if (isFolder) {
					const isOpen = expanded.has(node.path);
					return (
						<div key={node.path}>
							<div
								className="flex cursor-pointer items-center gap-1 py-[1.5px] pr-2 text-muted-foreground transition-colors hover:bg-accent/60"
								style={{ paddingLeft: pad }}
								onClick={() => onToggle(node.path)}
								onKeyDown={(event) => {
									if (event.key === "Enter" || event.key === " ") {
										onToggle(node.path);
									}
								}}
								tabIndex={0}
								role="treeitem"
								aria-expanded={isOpen}
							>
								<ChevronRight
									className={cn("size-3 shrink-0 transition-transform", isOpen && "rotate-90")}
									strokeWidth={2}
									aria-hidden
								/>
								<img
									src={getMaterialFolderIcon(node.name, isOpen || undefined)}
									alt=""
									className="size-3.5 shrink-0"
								/>
								<span className="truncate text-[11.5px]">{node.name}</span>
							</div>
							{isOpen ? (
								<TreeNodeList
									nodes={node.children}
									expanded={expanded}
									onToggle={onToggle}
									depth={depth + 1}
									group={group}
									workspaceRoot={workspaceRoot}
									gitBusy={gitBusy}
									runGit={runGit}
									flashingPaths={flashingPaths}
									selectedPath={selectedPath}
									onSelect={onSelect}
								/>
							) : null}
						</div>
					);
				}

				const file = node.file;
				if (!file) {
					return null;
				}

				return (
					<ChangeRow
						key={`${group}-${file.path}-${file.status}`}
						entry={file}
						group={group}
						workspaceRoot={workspaceRoot}
						gitBusy={gitBusy}
						runGit={runGit}
						treeIndentPx={depth * 12 + 22}
						fileIconSrc={getMaterialFileIcon(node.name)}
						flashingPaths={flashingPaths}
						selected={selectedPath === file.path}
						onSelect={onSelect}
					/>
				);
			})}
		</>
	);
}

function ShinyFlash({ active, children }: { active: boolean; children: React.ReactNode }) {
	const [shimmer, setShimmer] = useState(false);
	const counterRef = useRef(0);

	useEffect(() => {
		if (!active) return;
		counterRef.current += 1;
		setShimmer(true);
		const id = window.setTimeout(() => setShimmer(false), 3000);
		return () => window.clearTimeout(id);
	}, [active]);

	if (!shimmer) return <span className="truncate">{children}</span>;

	return (
		<AnimatedShinyText
			key={counterRef.current}
			shimmerWidth={60}
			className="!mx-0 !max-w-none truncate !text-neutral-500/80 ![animation-duration:1s] ![animation-iteration-count:3] ![animation-name:shiny-text-continuous] ![animation-timing-function:ease-in-out] dark:!text-neutral-500/80 dark:via-white via-black"
		>
			{children}
		</AnimatedShinyText>
	);
}

function BranchDiffSection({
	workspaceRoot,
	gitBusy,
	selectedPath = null,
	onSelect,
}: {
	workspaceRoot: string;
	gitBusy: boolean;
	selectedPath?: string | null;
	onSelect?: (selection: WorkspaceGitPreviewSelection) => void;
}) {
	const [open, setOpen] = useState(true);
	const [treeView, setTreeView] = useState(true);
	const query = useWorkspaceGitBranchDiff(workspaceRoot);
	const prevDataRef = useRef(query.data?.changes);
	const [flashingPaths, setFlashingPaths] = useState<Set<string>>(new Set());

	useEffect(() => {
		const prev = prevDataRef.current;
		const curr = query.data?.changes;
		prevDataRef.current = curr;
		if (!prev || !curr) return;
			const prevPaths = new Set(prev.map((entry: WorkspaceGitChangeEntry) => entry.path));
			const newPaths = curr
				.filter((entry: WorkspaceGitChangeEntry) => !prevPaths.has(entry.path))
				.map((entry: WorkspaceGitChangeEntry) => entry.path);
		if (newPaths.length === 0) return;
		setFlashingPaths(new Set(newPaths));
		const id = window.setTimeout(() => setFlashingPaths(new Set()), 3100);
		return () => window.clearTimeout(id);
	}, [query.data?.changes]);

	const changes = query.data?.changes ?? [];
	const baseBranch = query.data?.baseBranch ?? null;
	const loading = query.isPending;

	if (!loading && changes.length === 0) return null;

	return (
		<div className="border-b border-border/40 last:border-b-0">
			<div className="group/header flex w-full items-center gap-1 py-1 pl-1 pr-2 text-[11.5px] font-semibold tracking-[-0.01em] text-muted-foreground">
				<Button
					type="button"
					variant="ghost"
					size="xs"
					onClick={() => setOpen((v) => !v)}
					aria-expanded={open}
					className="h-auto min-w-0 flex-1 justify-start gap-1 rounded-none px-0 text-left hover:bg-transparent hover:text-foreground dark:hover:bg-transparent aria-expanded:bg-transparent aria-expanded:text-foreground"
				>
					<ChevronRight className={cn("size-3 shrink-0 transition-transform", open && "rotate-90")} strokeWidth={2} aria-hidden />
					<CloudIcon className="size-3 shrink-0 text-muted-foreground" strokeWidth={2} />
					<span className="truncate">Remote{baseBranch ? ` ← ${baseBranch.replace("origin/", "")}` : ""}</span>
				</Button>
				<RowIconButton
					aria-label={treeView ? "Switch to list view" : "Switch to tree view"}
					onClick={() => setTreeView((v) => !v)}
					className="text-transparent hover:bg-transparent group-hover/header:text-muted-foreground group-hover/header:hover:text-foreground"
				>
					{treeView ? <ListIcon className="size-3.5" strokeWidth={2} /> : <ListTree className="size-3.5" strokeWidth={2} />}
				</RowIconButton>
				<Badge variant="secondary" className="h-4 min-w-[16px] justify-center rounded-full px-1 text-[9.5px] font-semibold">
					{loading ? <LoaderCircleIcon className="size-2.5 animate-spin" /> : changes.length}
				</Badge>
			</div>
			{open && (
				<div className={cn("pb-2 pl-1 transition-opacity duration-150", loading && "pointer-events-none opacity-40")}>
					{treeView ? (
						<ChangesTreeView
							entries={changes}
							group="committed"
							workspaceRoot={workspaceRoot}
							gitBusy={gitBusy}
							runGit={async () => {}}
							flashingPaths={flashingPaths}
							selectedPath={selectedPath}
							onSelect={(selection) =>
								onSelect?.({ ...selection, baseBranch })
							}
						/>
					) : (
						<div className="pb-2 pl-1">
								{changes.map((entry: WorkspaceGitChangeEntry) => (
									<ChangeRow
										key={`remote-${entry.path}-${entry.status}`}
										entry={entry}
										group="committed"
										workspaceRoot={workspaceRoot}
										gitBusy={gitBusy}
									runGit={async () => {}}
									flashingPaths={flashingPaths}
									selected={selectedPath === entry.path}
									onSelect={(selection) =>
										onSelect?.({ ...selection, baseBranch })
									}
								/>
							))}
						</div>
					)}
				</div>
			)}
		</div>
	);
}

function ViewToggle({
	treeView,
	onToggle,
}: {
	treeView: boolean;
	onToggle: () => void;
}) {
	return (
		<RowIconButton
			aria-label={treeView ? "Switch to list view" : "Switch to tree view"}
			onClick={onToggle}
			className="text-transparent hover:bg-transparent group-hover/header:text-muted-foreground group-hover/header:hover:text-foreground"
		>
			{treeView ? (
				<ListIcon className="size-3.5" strokeWidth={2} />
			) : (
				<ListTree className="size-3.5" strokeWidth={2} />
			)}
		</RowIconButton>
	);
}

function ChangesGroup({
	label,
	count,
	open,
	onToggle,
	entries,
	group,
	workspaceRoot,
	gitBusy,
	runGit,
	onBatchAll,
	batchAriaLabel,
	BatchIcon,
	treeView,
	onToggleTreeView,
	showViewToggle,
	icon,
	selectedPath = null,
	onSelect,
}: {
	label: string;
	count: number;
	open: boolean;
	onToggle: () => void;
	entries: WorkspaceGitChangeEntry[];
	group: "staged" | "unstaged";
	workspaceRoot: string;
	gitBusy: boolean;
	runGit: (fn: () => Promise<void>) => Promise<void>;
	onBatchAll: () => Promise<void>;
	batchAriaLabel: string;
	BatchIcon: React.ComponentType<{ className?: string; strokeWidth?: number }>;
	treeView: boolean;
	onToggleTreeView: () => void;
	showViewToggle: boolean;
	icon?: React.ReactNode;
	selectedPath?: string | null;
	onSelect?: (selection: WorkspaceGitPreviewSelection) => void;
}) {
	return (
		<div className="border-b border-border/40 last:border-b-0">
			<div className="group/header flex w-full items-center gap-1 py-1 pl-1 pr-2 text-[11.5px] font-semibold tracking-[-0.01em] text-muted-foreground">
				<Button
					type="button"
					variant="ghost"
					size="xs"
					onClick={onToggle}
					aria-expanded={open}
					className="h-auto min-w-0 flex-1 justify-start gap-1 rounded-none px-0 text-left hover:bg-transparent hover:text-foreground dark:hover:bg-transparent aria-expanded:bg-transparent aria-expanded:text-foreground"
				>
					<ChevronRight
						className={cn("size-3 shrink-0 transition-transform", open && "rotate-90")}
						strokeWidth={2}
						aria-hidden
					/>
					{icon}
					<span className="truncate">{label}</span>
				</Button>
				{entries.length > 0 ? (
					<RowIconButton
						aria-label={batchAriaLabel}
						disabled={gitBusy}
						onClick={() => void onBatchAll()}
						className="text-transparent hover:bg-transparent group-hover/header:text-muted-foreground group-hover/header:hover:text-foreground"
					>
						<BatchIcon className="size-3.5" strokeWidth={2} />
					</RowIconButton>
				) : null}
				{showViewToggle ? (
					<ViewToggle treeView={treeView} onToggle={onToggleTreeView} />
				) : null}
				<Badge
					variant="secondary"
					className="h-4 min-w-[16px] justify-center rounded-full px-1 text-[9.5px] font-semibold"
				>
					{count}
				</Badge>
			</div>
			{open ? (
				treeView ? (
					<ChangesTreeView
						entries={entries}
						group={group}
						workspaceRoot={workspaceRoot}
						gitBusy={gitBusy}
						runGit={runGit}
						selectedPath={selectedPath}
						onSelect={onSelect}
					/>
				) : (
					<div className="pb-2 pl-1">
						{entries.map((e) => (
							<ChangeRow
								key={`${group}-${e.path}-${e.status}`}
								entry={e}
								group={group}
								workspaceRoot={workspaceRoot}
								gitBusy={gitBusy}
								runGit={runGit}
								selected={selectedPath === e.path}
								onSelect={onSelect}
							/>
						))}
					</div>
				)
			) : null}
		</div>
	);
}

type InspectorChangesSectionProps = {
	workspaceRoot: string | null;
	selectedPreview: WorkspaceGitPreviewSelection | null;
	onSelectPreview: (selection: WorkspaceGitPreviewSelection | null) => void;
	onPrefillComposer?: (text: string) => void;
};

export function InspectorChangesSection({
	workspaceRoot,
	selectedPreview,
	onSelectPreview,
	onPrefillComposer,
}: InspectorChangesSectionProps) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [stagedOpen, setStagedOpen] = useState(true);
	const [unstagedOpen, setUnstagedOpen] = useState(true);
	const [changesTreeView, setChangesTreeView] = useState(true);
	const [gitBusy, setGitBusy] = useState(false);

	const root = workspaceRoot?.trim() ?? "";

	const invalidateStatus = useCallback(async () => {
		await queryClient.invalidateQueries({
			queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
		});
	}, [queryClient, root]);

	const runGit = useCallback(
		async (fn: () => Promise<void>) => {
			setGitBusy(true);
			try {
				await fn();
				await invalidateStatus();
			} catch (error) {
				toast.error((error as Error)?.message ?? "Git command failed");
			} finally {
				setGitBusy(false);
			}
		},
		[invalidateStatus],
	);

	const query = useWorkspaceGitStatus(workspaceRoot);
	const branchDiffQuery = useWorkspaceGitBranchDiff(workspaceRoot);

	const unstageAll = useCallback(
		async (paths: string[]) => {
			await runGit(async () => {
				await Promise.all(
					paths.map((relativePath) =>
						workspaceGitUnstageFile({ workspaceRoot: root, relativePath }),
					),
				);
			});
		},
		[root, runGit],
	);

	const stageAll = useCallback(
		async () => {
			await runGit(async () => {
				await workspaceGitStageAll({
					workspaceRoot: root,
					relativePath: ".",
				});
			});
		},
		[root, runGit],
	);

	if (!root) {
		return (
			<p className="px-2 py-2 text-[11px] leading-5 text-muted-foreground">
				No workspace path — cannot read git changes.
			</p>
		);
	}

	if (query.isPending) {
		return (
			<p className="px-2 py-2 text-[11px] text-muted-foreground">Loading git status…</p>
		);
	}

	if (query.isError) {
		return (
			<p className="px-2 py-2 text-[11px] text-destructive">
				{(query.error as Error)?.message ?? "Git status failed"}
			</p>
		);
	}

	const data = query.data!;
	const hasAny = data.staged.length > 0 || data.unstaged.length > 0;

	return (
		<ScrollArea
			className="min-h-0 flex-1 bg-muted/15 font-mono text-[11.5px]"
			aria-label="Git changes"
		>
			<div className="min-w-0 max-w-full overflow-x-hidden pr-2">
				{data.staged.length > 0 ? (
					<ChangesGroup
						label="Staged changes"
						count={data.staged.length}
						open={stagedOpen}
						onToggle={() => setStagedOpen((v) => !v)}
						entries={data.staged}
						group="staged"
						workspaceRoot={root}
						gitBusy={gitBusy}
						runGit={runGit}
						onBatchAll={() => unstageAll(data.staged.map((e) => e.path))}
						batchAriaLabel="Unstage all changes"
						BatchIcon={MinusIcon}
						treeView={changesTreeView}
						onToggleTreeView={() => setChangesTreeView((v) => !v)}
						showViewToggle
						selectedPath={selectedPreview?.group === "staged" ? selectedPreview.path : null}
						onSelect={onSelectPreview}
					/>
				) : null}
				{data.unstaged.length > 0 ? (
					<ChangesGroup
						label="Changes"
						count={data.unstaged.length}
						open={unstagedOpen}
						onToggle={() => setUnstagedOpen((v) => !v)}
						entries={data.unstaged}
						group="unstaged"
						workspaceRoot={root}
						gitBusy={gitBusy}
						runGit={runGit}
						onBatchAll={stageAll}
						batchAriaLabel="Stage all changes"
						BatchIcon={PlusIcon}
						treeView={changesTreeView}
						onToggleTreeView={() => setChangesTreeView((v) => !v)}
						showViewToggle={data.staged.length === 0}
						icon={<LaptopIcon className="size-3 shrink-0 text-muted-foreground" strokeWidth={2} />}
						selectedPath={selectedPreview?.group === "unstaged" ? selectedPreview.path : null}
						onSelect={onSelectPreview}
					/>
				) : null}
				{!hasAny ? (
					<div className="px-3 py-3 text-[11px] leading-5 text-muted-foreground">
						{t("inspector.changes.emptyOnBranch")}
					</div>
				) : null}
				{Boolean(root) && (
					<BranchDiffSection
						workspaceRoot={root}
						gitBusy={gitBusy}
						selectedPath={selectedPreview?.group === "committed" ? selectedPreview.path : null}
						onSelect={onSelectPreview}
					/>
				)}
				{Boolean(root) ? (
					<CodeRabbitReviewSection
						workspaceRoot={root}
						staged={data.staged}
						unstaged={data.unstaged}
						baseBranch={branchDiffQuery.data?.baseBranch ?? null}
						onSelectPreview={onSelectPreview}
						onPrefillComposer={onPrefillComposer}
					/>
				) : null}
			</div>
		</ScrollArea>
	);
}
