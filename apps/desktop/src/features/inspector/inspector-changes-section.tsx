/**
 * Git changes list — staged / unstaged groups, list or tree view, per-file +/−,
 * extension icons and NumberTicker for diff stats.
 */

import { useQueryClient } from "@tanstack/react-query";
import { getMaterialFileIcon, getMaterialFolderIcon } from "file-extension-icon-js";
import {
	AlertCircle,
	Check,
	ChevronDown,
	ChevronRight,
	CloudIcon,
	Clock3,
	Expand,
	GitCompareArrows,
	LaptopIcon,
	List as ListIcon,
	ListTree,
	LoaderCircleIcon,
	MessageSquare,
	MinusIcon,
	PlusIcon,
	Undo2Icon,
} from "lucide-react";
import {
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { AnimatedShinyText } from "@/components/ui/animated-shiny-text";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { NumberTicker } from "@/components/ui/number-ticker";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { WorkspaceChangesDiffLoader } from "@/features/editor/WorkspaceChangesDiffLoader";
import type { WorkspaceGitChangeEntry, WorkspacePrReviewComment } from "@dcc/contracts";
import {
	workspaceGitDiscardFile,
	workspaceGitStageAll,
	workspaceGitStageFile,
	workspaceGitUnstageFile,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import { useCodeRabbitIntegrationEnabled } from "@/features/settings/coderabbit-preferences";
import { TurnReviewSurface } from "@/features/panel/turn-review-surface";
import { useCachedTurnReviewSummary } from "@/features/panel/turn-review-query";
import { CodeRabbitReviewSection } from "./coderabbit-review-section";
import { useWorkspaceGitStatus, WORKSPACE_GIT_STATUS_QUERY_KEY } from "./use-workspace-git-status";
import { useWorkspaceGitBranchDiff, WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY } from "./use-workspace-git-branch-diff";
import {
	WorkspaceGitFilePreview,
	type WorkspaceGitPreviewSelection,
} from "./workspace-git-file-preview";
import {
	availableInspectorReviewScopes,
	changeGroupBelongsToScope,
	reviewCardDiffHeight,
	resolveInspectorReviewScope,
	shouldEagerLoadReviewCard,
	summarizeInspectorChanges,
	type InspectorReviewScope,
} from "./inspector-changes-presentation";
import { useWorkspaceGitFilePreviewContent } from "./use-workspace-git-file-preview-content";

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
const EMPTY_REVIEW_COMMENTS_BY_PATH = new Map<string, WorkspacePrReviewComment[]>();

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
	reviewCommentCount = 0,
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
	reviewCommentCount?: number;
	onSelect?: (selection: WorkspaceGitPreviewSelection) => void;
}) {
	const folder = dirname(entry.path);
	const input = { workspaceRoot, relativePath: entry.path };
	const iconSrc = fileIconSrc ?? getMaterialFileIcon(entry.name);
	const unmerged = /^(U|AA|DD|AU|UA|DU|UD)$/.test(entry.status.toUpperCase());

	return (
		<div
			className={cn(
				"group/row relative mx-1 flex min-h-8 items-center gap-2 rounded-md py-1 pl-2 pr-2 text-[11.5px] text-muted-foreground transition-[background-color,color,box-shadow] hover:bg-accent/55",
				onSelect && "cursor-pointer",
				selected && "bg-primary/[0.08] text-foreground shadow-[inset_2px_0_0_var(--primary)]",
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
			<span className="min-w-0 max-w-[46%] truncate font-medium text-foreground sm:max-w-[58%]">
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
				{reviewCommentCount > 0 ? (
					<span
						className="inline-flex h-4 items-center gap-0.5 rounded-full bg-primary/10 px-1 text-[9.5px] font-semibold text-primary"
						title={`${reviewCommentCount} review comment${reviewCommentCount === 1 ? "" : "s"}`}
					>
						<MessageSquare className="size-2.5" strokeWidth={2} />
						{reviewCommentCount}
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
			) : group === "unstaged" && !unmerged ? (
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
	reviewCommentCounts,
	onSelect,
}: {
	entries: WorkspaceGitChangeEntry[];
	group: "staged" | "unstaged" | "committed";
	workspaceRoot: string;
	gitBusy: boolean;
	runGit: (fn: () => Promise<void>) => Promise<void>;
	flashingPaths?: Set<string>;
	selectedPath?: string | null;
	reviewCommentCounts?: Map<string, number>;
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
				reviewCommentCounts={reviewCommentCounts}
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
	reviewCommentCounts,
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
	reviewCommentCounts?: Map<string, number>;
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
									reviewCommentCounts={reviewCommentCounts}
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
						reviewCommentCount={reviewCommentCounts?.get(file.path) ?? 0}
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

function ReviewChangeCard({
	entry,
	group,
	workspaceRoot,
	baseBranch = null,
	index,
	gitBusy,
	runGit,
	reviewCommentCount = 0,
	onExpand,
}: {
	entry: WorkspaceGitChangeEntry;
	group: "staged" | "unstaged" | "committed";
	workspaceRoot: string;
	baseBranch?: string | null;
	index: number;
	gitBusy: boolean;
	runGit: (fn: () => Promise<void>) => Promise<void>;
	reviewCommentCount?: number;
	onExpand?: (selection: WorkspaceGitPreviewSelection) => void;
}) {
	const { t } = useTranslation("common");
	const cardRef = useRef<HTMLElement | null>(null);
	const [open, setOpen] = useState(true);
	const [shouldLoad, setShouldLoad] = useState(() =>
		shouldEagerLoadReviewCard(index),
	);
	const input = { workspaceRoot, relativePath: entry.path };
	const unmerged = /^(U|AA|DD|AU|UA|DU|UD)$/.test(
		entry.status.toUpperCase(),
	);
	const selection = useMemo<WorkspaceGitPreviewSelection>(
		() => ({
			group,
			path: entry.path,
			name: entry.name,
			status: entry.status,
			baseBranch,
		}),
		[baseBranch, entry.name, entry.path, entry.status, group],
	);

	useEffect(() => {
		if (shouldLoad || !open) return;
		if (typeof IntersectionObserver === "undefined") {
			setShouldLoad(true);
			return;
		}
		const target = cardRef.current;
		if (!target) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (!entries.some((candidate) => candidate.isIntersecting)) return;
				setShouldLoad(true);
				observer.disconnect();
			},
			{ rootMargin: "700px 0px" },
		);
		observer.observe(target);
		return () => observer.disconnect();
	}, [open, shouldLoad]);

	const query = useWorkspaceGitFilePreviewContent(
		shouldLoad && open
			? {
					workspaceRoot,
					relativePath: entry.path,
					status: entry.status,
					scope: group,
					baseBranch,
				}
			: null,
	);
	const iconSrc = getMaterialFileIcon(entry.name);
	const folder = dirname(entry.path);

	return (
		<article
			ref={cardRef}
			className="overflow-hidden rounded-xl border border-border/60 bg-background shadow-[0_1px_2px_rgba(0,0,0,0.08)]"
		>
			<div className="flex min-h-11 items-center gap-2 border-b border-border/45 px-2.5 py-1.5">
				<button
					type="button"
					className="flex min-w-0 flex-1 items-center gap-2 rounded-md text-left outline-none focus-visible:ring-1 focus-visible:ring-ring"
					onClick={() => setOpen((value) => !value)}
					aria-expanded={open}
				>
					{open ? (
						<ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
					) : (
						<ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
					)}
					<img src={iconSrc} alt="" className="size-3.5 shrink-0" />
					<span className="min-w-0 flex-1 truncate">
						<span className="font-medium text-foreground">{entry.name}</span>
						{folder ? (
							<span className="ml-1.5 text-[10px] text-muted-foreground">
								{folder}
							</span>
						) : null}
					</span>
				</button>
				<div className="flex shrink-0 items-center gap-1 text-[10px] tabular-nums">
					{entry.insertions > 0 ? (
						<span className="text-emerald-600 dark:text-emerald-400">
							+{entry.insertions}
						</span>
					) : null}
					{entry.deletions > 0 ? (
						<span className="text-destructive">−{entry.deletions}</span>
					) : null}
					{reviewCommentCount > 0 ? (
						<span className="inline-flex items-center gap-0.5 rounded-full bg-primary/10 px-1.5 py-0.5 font-semibold text-primary">
							<MessageSquare className="size-2.5" />
							{reviewCommentCount}
						</span>
					) : null}
					<span
						className={cn(
							"min-w-4 text-center font-semibold",
							statusClass(entry.status),
						)}
					>
						{entry.status}
					</span>
					{group === "staged" ? (
						<RowIconButton
							aria-label={t("inspector.changes.unstageFile")}
							disabled={gitBusy}
							onClick={() =>
								void runGit(() => workspaceGitUnstageFile(input))
							}
						>
							<MinusIcon className="size-3.5" />
						</RowIconButton>
					) : group === "unstaged" && !unmerged ? (
						<>
							<RowIconButton
								aria-label={t("inspector.changes.discardFile")}
								disabled={gitBusy}
								onClick={() =>
									void runGit(() => workspaceGitDiscardFile(input))
								}
							>
								<Undo2Icon className="size-3.5" />
							</RowIconButton>
							<RowIconButton
								aria-label={t("inspector.changes.stageFile")}
								disabled={gitBusy}
								onClick={() =>
									void runGit(() => workspaceGitStageFile(input))
								}
							>
								<PlusIcon className="size-3.5" />
							</RowIconButton>
						</>
					) : null}
					{onExpand ? (
						<RowIconButton
							aria-label={t("inspector.changes.expandDiff")}
							onClick={() => onExpand(selection)}
						>
							<Expand className="size-3.5" />
						</RowIconButton>
					) : null}
				</div>
			</div>
			{open ? (
				<div
					className="min-h-[190px] overflow-hidden bg-background"
					style={{
						height: `${reviewCardDiffHeight(entry.insertions, entry.deletions)}px`,
					}}
				>
					{!shouldLoad || query.isPending ? (
						<div className="flex h-full items-center justify-center gap-2 text-[11px] text-muted-foreground">
							<LoaderCircleIcon className="size-3.5 animate-spin" />
							{t("inspector.changes.loadingDiff")}
						</div>
					) : query.isError ? (
						<div className="flex h-full items-center justify-center gap-2 px-4 text-center text-[11px] text-destructive">
							<AlertCircle className="size-3.5" />
							{(query.error as Error)?.message ??
								t("inspector.changes.diffFailed")}
						</div>
					) : query.data ? (
						<WorkspaceChangesDiffLoader
							path={entry.path}
							originalText={query.data.originalText}
							modifiedText={query.data.modifiedText}
							inline
							className="h-full"
						/>
					) : null}
				</div>
			) : null}
		</article>
	);
}

function BranchDiffSection({
	workspaceRoot,
	gitBusy,
	treeView,
	selectedPath = null,
	reviewCommentCounts,
	onExpand,
}: {
	workspaceRoot: string;
	gitBusy: boolean;
	treeView: boolean;
	selectedPath?: string | null;
	reviewCommentCounts?: Map<string, number>;
	onExpand?: (selection: WorkspaceGitPreviewSelection) => void;
}) {
	const query = useWorkspaceGitBranchDiff(workspaceRoot);
	const prevDataRef = useRef(query.data?.changes);
	const [flashingPaths, setFlashingPaths] = useState<Set<string>>(new Set());

	useEffect(() => {
		const prev = prevDataRef.current;
		const curr = query.data?.changes;
		prevDataRef.current = curr;
		if (!prev || !curr) return;
		const prevPaths = new Set(
			prev.map((entry: WorkspaceGitChangeEntry) => entry.path),
		);
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
		<div className="pb-2">
			<div className="flex min-h-8 items-center gap-2 border-b border-border/35 px-3 text-[10.5px] text-muted-foreground">
				<CloudIcon className="size-3.5 shrink-0" strokeWidth={1.8} />
				<span className="min-w-0 flex-1 truncate">
					{baseBranch ? baseBranch.replace("origin/", "") : "Remote"}
				</span>
				{loading ? <LoaderCircleIcon className="size-3 animate-spin" /> : null}
			</div>
			<div
				className={cn(
					"pt-1 transition-opacity duration-150",
					loading && "pointer-events-none opacity-40",
				)}
			>
				{treeView ? (
					<ChangesTreeView
						entries={changes}
						group="committed"
						workspaceRoot={workspaceRoot}
						gitBusy={gitBusy}
						runGit={async () => {}}
						flashingPaths={flashingPaths}
						selectedPath={selectedPath}
						reviewCommentCounts={reviewCommentCounts}
						onSelect={(selection) =>
							onExpand?.({ ...selection, baseBranch })
						}
					/>
				) : (
					<div className="space-y-2 px-2 pb-2">
						{changes.map((entry: WorkspaceGitChangeEntry, index: number) => (
							<ReviewChangeCard
								key={`remote-${entry.path}-${entry.status}`}
								entry={entry}
								group="committed"
								workspaceRoot={workspaceRoot}
								baseBranch={baseBranch}
								index={index}
								gitBusy={gitBusy}
								runGit={async () => {}}
								reviewCommentCount={reviewCommentCounts?.get(entry.path) ?? 0}
								onExpand={onExpand}
							/>
						))}
					</div>
				)}
			</div>
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
	batchDisabled = false,
	batchAriaLabel,
	BatchIcon,
	treeView,
	onToggleTreeView,
	showViewToggle,
	icon,
	selectedPath = null,
	reviewCommentCounts,
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
	batchDisabled?: boolean;
	batchAriaLabel: string;
	BatchIcon: React.ComponentType<{ className?: string; strokeWidth?: number }>;
	treeView: boolean;
	onToggleTreeView: () => void;
	showViewToggle: boolean;
	icon?: React.ReactNode;
	selectedPath?: string | null;
	reviewCommentCounts?: Map<string, number>;
	onSelect?: (selection: WorkspaceGitPreviewSelection) => void;
}) {
	return (
		<div className="border-b border-border/35 last:border-b-0">
			<div className="group/header flex min-h-9 w-full items-center gap-1 px-2 text-[10.5px] font-semibold uppercase tracking-[0.055em] text-muted-foreground">
				<Button
					type="button"
					variant="ghost"
					size="xs"
					onClick={onToggle}
					aria-expanded={open}
					className="h-auto min-w-0 flex-1 justify-start gap-1.5 rounded-none px-0 text-left hover:bg-transparent hover:text-foreground dark:hover:bg-transparent aria-expanded:bg-transparent aria-expanded:text-foreground"
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
						disabled={gitBusy || batchDisabled}
						onClick={() => void onBatchAll()}
						className="text-transparent hover:bg-transparent group-hover/header:text-muted-foreground group-hover/header:hover:text-foreground"
					>
						<BatchIcon className="size-3.5" strokeWidth={2} />
					</RowIconButton>
				) : null}
				{showViewToggle ? (
					<ViewToggle treeView={treeView} onToggle={onToggleTreeView} />
				) : null}
				<Badge variant="secondary" className="h-5 min-w-5 justify-center rounded-full px-1.5 text-[9.5px] font-semibold">
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
						reviewCommentCounts={reviewCommentCounts}
						onSelect={onSelect}
					/>
				) : (
					<div className="space-y-2 px-2 pb-2">
						{entries.map((e, index) => (
							<ReviewChangeCard
								key={`${group}-${e.path}-${e.status}`}
								entry={e}
								group={group}
								workspaceRoot={workspaceRoot}
								index={index}
								gitBusy={gitBusy}
								runGit={runGit}
								reviewCommentCount={reviewCommentCounts?.get(e.path) ?? 0}
								onExpand={onSelect}
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
	workspaceId: string | null;
	sessionId: string | null;
	lastTurnReviewRequest?: { sessionId: string; nonce: number } | null;
	selectedPreview: WorkspaceGitPreviewSelection | null;
	onSelectPreview: (selection: WorkspaceGitPreviewSelection | null) => void;
	onPrefillComposer?: (text: string) => void;
	reviewCommentsByPath?: Map<string, WorkspacePrReviewComment[]>;
	targetSessionId?: string | null;
	onOpenExpandedPreview?: (selection?: WorkspaceGitPreviewSelection) => void;
	/** A commit/push/PR action is reconciling the Git state shown below. */
	isGitActionInProgress?: boolean;
};

export function InspectorChangesSection({
	workspaceRoot,
	workspaceId,
	sessionId,
	lastTurnReviewRequest = null,
	selectedPreview,
	onSelectPreview,
	onPrefillComposer,
	reviewCommentsByPath = EMPTY_REVIEW_COMMENTS_BY_PATH,
	targetSessionId = null,
	onOpenExpandedPreview,
	isGitActionInProgress = false,
}: InspectorChangesSectionProps) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [stagedOpen, setStagedOpen] = useState(true);
	const [unstagedOpen, setUnstagedOpen] = useState(true);
	const [changesTreeView, setChangesTreeView] = useState(false);
	const [scopePreference, setScopePreference] = useState<{
		root: string;
		scope: InspectorReviewScope;
	} | null>(null);
	const [gitBusy, setGitBusy] = useState(false);
	const [discardAllDialogOpen, setDiscardAllDialogOpen] = useState(false);
	const codeRabbitEnabled = useCodeRabbitIntegrationEnabled();
	const lastTurnReview = useCachedTurnReviewSummary(sessionId, workspaceId);
	const handledLastTurnReviewRequestRef = useRef<number | null>(null);

	const root = workspaceRoot?.trim() ?? "";
	useEffect(() => {
		if (
			!lastTurnReviewRequest ||
			lastTurnReviewRequest.sessionId !== sessionId ||
			handledLastTurnReviewRequestRef.current === lastTurnReviewRequest.nonce
		) {
			return;
		}
		handledLastTurnReviewRequestRef.current = lastTurnReviewRequest.nonce;
		setScopePreference({ root, scope: "last-turn" });
		onSelectPreview(null);
	}, [lastTurnReviewRequest, onSelectPreview, root, sessionId]);

	useEffect(() => {
		if (
			codeRabbitEnabled ||
			!selectedPreview?.machineAnnotations?.some(
				(annotation) => annotation.source === "coderabbit",
			)
		) {
			return;
		}
		onSelectPreview({
			...selectedPreview,
			machineAnnotations: undefined,
		});
	}, [codeRabbitEnabled, onSelectPreview, selectedPreview]);

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
	const reviewCommentCounts = useMemo(() => {
		const counts = new Map<string, number>();
		for (const [path, comments] of reviewCommentsByPath) {
			if (comments.length > 0) {
				counts.set(path, comments.length);
			}
		}
		return counts;
	}, [reviewCommentsByPath]);
	const enrichPreviewSelection = useCallback(
		(selection: WorkspaceGitPreviewSelection) => ({
			...selection,
			workspaceRootOverride: selection.workspaceRootOverride ?? root,
			targetSessionId: selection.targetSessionId ?? targetSessionId,
			reviewComments: reviewCommentsByPath.get(selection.path) ?? [],
		}),
		[reviewCommentsByPath, root, targetSessionId],
	);
	const handleSelectPreview = useCallback(
		(selection: WorkspaceGitPreviewSelection | null) => {
			if (!selection) {
				onSelectPreview(null);
				return;
			}
			onSelectPreview(enrichPreviewSelection(selection));
		},
		[enrichPreviewSelection, onSelectPreview],
	);
	const handleExpandPreview = useCallback(
		(selection: WorkspaceGitPreviewSelection) => {
			onOpenExpandedPreview?.(enrichPreviewSelection(selection));
		},
		[enrichPreviewSelection, onOpenExpandedPreview],
	);
	const handleScopeChange = useCallback(
		(scope: InspectorReviewScope) => {
			setScopePreference({ root, scope });
			if (
				selectedPreview &&
				!changeGroupBelongsToScope(selectedPreview.group, scope)
			) {
				onSelectPreview(null);
			}
		},
		[onSelectPreview, root, selectedPreview],
	);

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
	const discardAllWorkingChanges = useCallback(
		async (stagedPaths: string[], allPaths: string[]) => {
			await runGit(async () => {
				for (const relativePath of stagedPaths) {
					await workspaceGitUnstageFile({ workspaceRoot: root, relativePath });
				}
				for (const relativePath of [...new Set(allPaths)]) {
					await workspaceGitDiscardFile({ workspaceRoot: root, relativePath });
				}
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
			<div
				className="flex min-h-0 flex-1 items-center justify-center gap-2 px-3 py-6 text-[11px] text-muted-foreground"
				role="status"
				aria-live="polite"
			>
				<LoaderCircleIcon className="size-3.5 animate-spin" aria-hidden />
				{t("inspector.changes.loadingGit")}
			</div>
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
	const workingChanges = [...data.staged, ...data.unstaged];
	const branchChanges = branchDiffQuery.data?.changes ?? [];
	const preferredScope =
		scopePreference?.root === root ? scopePreference.scope : null;
	const activeScope = resolveInspectorReviewScope(
		preferredScope,
		Boolean(sessionId && workspaceId),
		workingChanges.length,
		branchChanges.length,
	);
	const visibleChanges =
		activeScope === "working"
			? workingChanges
			: activeScope === "branch"
				? branchChanges
				: [];
	const visibleSummary =
		activeScope === "last-turn" && lastTurnReview
			? {
					fileCount: lastTurnReview.files.length,
					insertions: lastTurnReview.insertions,
					deletions: lastTurnReview.deletions,
				}
			: summarizeInspectorChanges(visibleChanges);
	const reviewScopes = availableInspectorReviewScopes(
		Boolean(sessionId && workspaceId),
	);
	const scopeCounts: Record<InspectorReviewScope, number> = {
		working: workingChanges.length,
		"last-turn": lastTurnReview?.files.length ?? 0,
		branch: branchChanges.length,
	};
	const ActiveScopeIcon =
		activeScope === "working"
			? LaptopIcon
			: activeScope === "last-turn"
				? Clock3
				: GitCompareArrows;
	const hasReviewableChanges = hasAny || branchChanges.length > 0;

	if (selectedPreview) {
		return (
			<div className="relative flex min-h-0 flex-1 flex-col">
				<WorkspaceGitFilePreview
					workspaceRoot={root}
					selection={selectedPreview}
					onBack={() => handleSelectPreview(null)}
					onExpand={() => onOpenExpandedPreview?.(selectedPreview)}
					forceUnified
				/>
				{isGitActionInProgress ? (
					<div
						className="absolute right-2 top-2 z-20 flex items-center gap-2 rounded-md border border-border/60 bg-background/95 px-2 py-1 text-[10px] font-medium text-muted-foreground shadow-sm backdrop-blur-[1px]"
						role="status"
						aria-live="polite"
					>
						<LoaderCircleIcon className="size-3.5 animate-spin text-primary" aria-hidden />
						{t("inspector.changes.updatingGit")}
					</div>
				) : null}
			</div>
		);
	}

	return (
		<div className="relative flex min-h-0 flex-1 flex-col">
			<div className="shrink-0 border-b border-border/50 bg-background">
				<div className="flex min-h-10 items-center gap-1 px-2 py-1.5">
					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="h-8 min-w-0 flex-1 justify-start gap-2 rounded-lg px-2 text-[11px] font-medium hover:bg-muted/50"
								aria-label={t("inspector.changes.scopeLabel")}
							>
								<ActiveScopeIcon className="size-3.5 shrink-0" strokeWidth={1.9} />
								<span className="truncate">
									{t(`inspector.changes.scopes.${activeScope}`)}
								</span>
								<span className="tabular-nums text-muted-foreground">
									{scopeCounts[activeScope]}
								</span>
								<ChevronDown className="ml-auto size-3.5 shrink-0 text-muted-foreground" />
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="start" className="min-w-52">
							{reviewScopes.map((scope) => {
								const Icon =
									scope === "working"
										? LaptopIcon
										: scope === "last-turn"
											? Clock3
											: GitCompareArrows;
								return (
									<DropdownMenuItem
										key={scope}
										size="sm"
										className="h-8 gap-2"
										onSelect={() => handleScopeChange(scope)}
									>
										<Icon className="size-3.5 shrink-0" strokeWidth={1.9} />
										<span className="min-w-0 flex-1 truncate">
											{t(`inspector.changes.scopes.${scope}`)}
										</span>
										<span className="tabular-nums text-muted-foreground">
											{scopeCounts[scope]}
										</span>
										{scope === activeScope ? <Check className="size-3.5" /> : null}
									</DropdownMenuItem>
								);
							})}
						</DropdownMenuContent>
					</DropdownMenu>
					<div
						className="flex shrink-0 items-center gap-1 text-[10px] tabular-nums"
						aria-label={t("inspector.changes.fileCount", {
							count: visibleSummary.fileCount,
						})}
					>
						<span className="text-emerald-600 dark:text-emerald-400">+{visibleSummary.insertions}</span>
						<span className="text-destructive">−{visibleSummary.deletions}</span>
					</div>
					{activeScope !== "last-turn" ? (
						<Tooltip>
							<TooltipTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="icon-xs"
									className="size-7 shrink-0 text-muted-foreground hover:text-foreground"
									aria-label={t(
										changesTreeView
											? "inspector.changes.listView"
											: "inspector.changes.treeView",
									)}
									onClick={() => setChangesTreeView((value) => !value)}
								>
									{changesTreeView ? (
										<ListIcon className="size-4" />
									) : (
										<ListTree className="size-4" />
									)}
								</Button>
							</TooltipTrigger>
							<TooltipContent side="bottom">
								{t(
									changesTreeView
										? "inspector.changes.listView"
										: "inspector.changes.treeView",
								)}
							</TooltipContent>
						</Tooltip>
					) : null}
				</div>
			</div>
			{activeScope === "last-turn" && sessionId && workspaceId ? (
				<TurnReviewSurface sessionId={sessionId} workspaceId={workspaceId} />
			) : (
				<ScrollArea
					viewportProps={{ "data-inspector-scroll-key": "git-changes" }}
					className="min-h-0 flex-1 bg-muted/[0.08] text-[11.5px]"
					aria-label="Git changes"
				>
					<div
						className={cn(
							"min-w-0 max-w-full overflow-x-hidden py-1",
							activeScope === "working" && hasAny && "pb-16",
						)}
					>
					{activeScope === "working" && data.staged.length > 0 ? (
						<ChangesGroup
							label={t("inspector.changes.groups.staged")}
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
							showViewToggle={false}
							selectedPath={null}
							reviewCommentCounts={reviewCommentCounts}
							onSelect={handleExpandPreview}
						/>
					) : null}
					{activeScope === "working" && data.unstaged.length > 0 ? (
						<ChangesGroup
							label={t("inspector.changes.groups.unstaged")}
							count={data.unstaged.length}
							open={unstagedOpen}
							onToggle={() => setUnstagedOpen((v) => !v)}
							entries={data.unstaged}
							group="unstaged"
							workspaceRoot={root}
							gitBusy={gitBusy}
							runGit={runGit}
							onBatchAll={stageAll}
							batchDisabled={data.conflictCount > 0}
							batchAriaLabel="Stage all changes"
							BatchIcon={PlusIcon}
							treeView={changesTreeView}
							onToggleTreeView={() => setChangesTreeView((v) => !v)}
							showViewToggle={false}
							icon={
								<LaptopIcon
									className="size-3 shrink-0 text-muted-foreground"
									strokeWidth={2}
								/>
							}
							selectedPath={null}
							reviewCommentCounts={reviewCommentCounts}
							onSelect={handleExpandPreview}
						/>
					) : null}
					{activeScope === "working" && !hasAny ? (
						<div className="flex min-h-36 flex-col items-center justify-center px-5 py-8 text-center text-[11px] leading-5 text-muted-foreground">
							<div className="mb-2 flex size-8 items-center justify-center rounded-full bg-muted/60">
								<GitCompareArrows className="size-4" strokeWidth={1.7} />
							</div>
							{t("inspector.changes.emptyWorking")}
						</div>
					) : null}
					{activeScope === "branch" &&
					!branchDiffQuery.isPending &&
					branchChanges.length === 0 ? (
						<div className="flex min-h-36 flex-col items-center justify-center px-5 py-8 text-center text-[11px] leading-5 text-muted-foreground">
							<div className="mb-2 flex size-8 items-center justify-center rounded-full bg-muted/60">
								<CloudIcon className="size-4" strokeWidth={1.7} />
							</div>
							{t("inspector.changes.emptyBranch")}
						</div>
					) : null}
					{activeScope === "branch" && Boolean(root) ? (
						<BranchDiffSection
							workspaceRoot={root}
							gitBusy={gitBusy}
							treeView={changesTreeView}
							selectedPath={null}
							reviewCommentCounts={reviewCommentCounts}
							onExpand={handleExpandPreview}
						/>
					) : null}
					{codeRabbitEnabled &&
					hasReviewableChanges &&
					visibleChanges.length > 0 ? (
						<CodeRabbitReviewSection
							workspaceRoot={root}
							staged={data.staged}
							unstaged={data.unstaged}
							baseBranch={branchDiffQuery.data?.baseBranch ?? null}
							onSelectPreview={handleSelectPreview}
							onPrefillComposer={onPrefillComposer}
						/>
					) : null}
				</div>
				</ScrollArea>
			)}
			{activeScope === "working" && hasAny ? (
				<div className="pointer-events-none absolute inset-x-0 bottom-3 z-20 flex justify-center px-3">
					<div className="pointer-events-auto flex items-center rounded-full border border-border/70 bg-background/95 p-1 shadow-lg backdrop-blur">
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="h-8 rounded-full gap-1.5 px-3"
							disabled={gitBusy || data.conflictCount > 0}
							onClick={() => setDiscardAllDialogOpen(true)}
						>
							<Undo2Icon className="size-3.5" />
							{t("inspector.changes.revertAll")}
						</Button>
						<div className="mx-0.5 h-5 w-px bg-border/70" aria-hidden />
						{data.unstaged.length > 0 ? (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="h-8 rounded-full gap-1.5 px-3"
								disabled={gitBusy || data.conflictCount > 0}
								onClick={() => void stageAll()}
							>
								<PlusIcon className="size-3.5" />
								{t("inspector.changes.stageAll")}
							</Button>
						) : (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="h-8 rounded-full gap-1.5 px-3"
								disabled={gitBusy}
								onClick={() =>
									void unstageAll(data.staged.map((entry) => entry.path))
								}
							>
								<MinusIcon className="size-3.5" />
								{t("inspector.changes.unstageAll")}
							</Button>
						)}
					</div>
				</div>
			) : null}
			{isGitActionInProgress ? (
				<div
					className="absolute right-2 top-2 z-20 flex items-center gap-2 rounded-md border border-border/60 bg-background/95 px-2 py-1 text-[10px] font-medium text-muted-foreground shadow-sm backdrop-blur-[1px]"
					role="status"
					aria-live="polite"
				>
					<LoaderCircleIcon className="size-3.5 animate-spin text-primary" aria-hidden />
					{t("inspector.changes.updatingGit")}
				</div>
			) : null}
			<Dialog open={discardAllDialogOpen} onOpenChange={setDiscardAllDialogOpen}>
				<DialogContent className="sm:max-w-md">
					<DialogHeader>
						<DialogTitle>{t("inspector.changes.revertAllConfirmTitle")}</DialogTitle>
						<DialogDescription>
							{t("inspector.changes.revertAllConfirmDescription", {
								count: workingChanges.length,
							})}
						</DialogDescription>
					</DialogHeader>
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => setDiscardAllDialogOpen(false)}
						>
							{t("inspector.changes.cancel")}
						</Button>
						<Button
							type="button"
							variant="destructive"
							disabled={gitBusy}
							onClick={() => {
								setDiscardAllDialogOpen(false);
								void discardAllWorkingChanges(
									data.staged.map((entry) => entry.path),
									workingChanges.map((entry) => entry.path),
								);
							}}
						>
							{t("inspector.changes.revertAllConfirmAction")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</div>
	);
}
