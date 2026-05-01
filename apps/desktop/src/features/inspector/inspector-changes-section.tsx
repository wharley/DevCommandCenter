/**
 * Git changes list — staged / unstaged groups, list or tree view, per-file +/−,
 * extension icons (Helmor-style), NumberTicker for diff stats.
 */

import { useQueryClient } from "@tanstack/react-query";
import { getMaterialFileIcon, getMaterialFolderIcon } from "file-extension-icon-js";
import {
	ChevronRight,
	List as ListIcon,
	ListTree,
	MinusIcon,
	PlusIcon,
	Undo2Icon,
} from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { NumberTicker } from "@/components/ui/number-ticker";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { WorkspaceGitChangeEntry } from "@dcc/contracts";
import {
	workspaceGitDiscardFile,
	workspaceGitStageFile,
	workspaceGitUnstageFile,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import { useWorkspaceGitStatus, WORKSPACE_GIT_STATUS_QUERY_KEY } from "./use-workspace-git-status";

export { WORKSPACE_GIT_STATUS_QUERY_KEY };

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
}: {
	entry: WorkspaceGitChangeEntry;
	group: "staged" | "unstaged";
	workspaceRoot: string;
	gitBusy: boolean;
	runGit: (fn: () => Promise<void>) => Promise<void>;
	treeIndentPx?: number;
	fileIconSrc?: string;
}) {
	const folder = dirname(entry.path);
	const input = { workspaceRoot, relativePath: entry.path };
	const iconSrc = fileIconSrc ?? getMaterialFileIcon(entry.name);

	return (
		<div
			className="group/row flex cursor-default items-center gap-1.5 py-[1.5px] pl-2 pr-2 text-[11.5px] text-muted-foreground transition-colors hover:bg-accent/60"
			style={treeIndentPx > 0 ? { paddingLeft: treeIndentPx } : undefined}
			title={entry.absolutePath}
		>
			<img src={iconSrc} alt="" className="size-3.5 shrink-0" />
			<span className="min-w-0 max-w-[40%] truncate font-medium text-foreground sm:max-w-[52%]">
				{entry.name}
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
				<span className="ml-auto hidden items-center gap-0.5 group-hover/row:inline-flex">
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
			) : (
				<span className="ml-auto hidden items-center gap-0.5 group-hover/row:inline-flex">
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
			)}
		</div>
	);
}

function ChangesTreeView({
	entries,
	group,
	workspaceRoot,
	gitBusy,
	runGit,
}: {
	entries: WorkspaceGitChangeEntry[];
	group: "staged" | "unstaged";
	workspaceRoot: string;
	gitBusy: boolean;
	runGit: (fn: () => Promise<void>) => Promise<void>;
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
}: {
	nodes: Map<string, TreeNode>;
	expanded: Set<string>;
	onToggle: (path: string) => void;
	depth: number;
	group: "staged" | "unstaged";
	workspaceRoot: string;
	gitBusy: boolean;
	runGit: (fn: () => Promise<void>) => Promise<void>;
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
					/>
				);
			})}
		</>
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
};

export function InspectorChangesSection({ workspaceRoot }: InspectorChangesSectionProps) {
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
		async (paths: string[]) => {
			await runGit(async () => {
				await Promise.all(
					paths.map((relativePath) =>
						workspaceGitStageFile({ workspaceRoot: root, relativePath }),
					),
				);
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
			<div className="pr-2">
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
						onBatchAll={() => stageAll(data.unstaged.map((e) => e.path))}
						batchAriaLabel="Stage all changes"
						BatchIcon={PlusIcon}
						treeView={changesTreeView}
						onToggleTreeView={() => setChangesTreeView((v) => !v)}
						showViewToggle={data.staged.length === 0}
					/>
				) : null}
				{!hasAny ? (
					<div className="px-3 py-3 text-[11px] leading-5 text-muted-foreground">
						No changes on this branch.
					</div>
				) : null}
			</div>
		</ScrollArea>
	);
}
