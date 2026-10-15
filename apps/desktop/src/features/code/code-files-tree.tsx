import type { WorkspacePrReviewComment } from "@dcc/contracts";
import { useQuery } from "@tanstack/react-query";
import { getMaterialFileIcon, getMaterialFolderIcon } from "file-extension-icon-js";
import { ChevronRight, Loader2, MessageSquare, Search } from "lucide-react";
import {
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
	type KeyboardEvent as ReactKeyboardEvent,
	type MouseEvent as ReactMouseEvent,
	type PointerEvent as ReactPointerEvent,
} from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { InlineShortcutDisplay } from "@/features/shortcuts/InlineShortcutDisplay";
import { getQuickOpenShortcutKeys } from "@/features/shortcuts/shortcut-utils";
import { listGitTrackedFiles } from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import { useWorkspaceGitStatus } from "@/features/inspector/use-workspace-git-status";
import {
	ancestorFolderPaths,
	buildCodeTree,
	codeTreeGitStatus,
	fileSetSignature,
	visibleCodeTreeRows,
	type CodeTreeGitStatus,
	type CodeTreeRow,
} from "./code-files-tree.logic";

const EMPTY_CODE_FILE_PATHS: string[] = [];
const EMPTY_REVIEW_COMMENTS = new Map<string, WorkspacePrReviewComment[]>();
const EMPTY_CHANGES: { path: string; status: string }[] = [];
/** Expanded folders per worktree, kept across remounts of the tree. */
const codeTreeExpansionCache = new Map<string, Set<string>>();

const GIT_TONE_CLASS: Record<CodeTreeGitStatus["tone"], string> = {
	modified: "text-amber-500",
	added: "text-emerald-500",
	deleted: "text-red-500",
	renamed: "text-sky-500",
	conflict: "text-red-500",
};

function basename(path: string): string {
	const slash = path.lastIndexOf("/");
	return slash === -1 ? path : path.slice(slash + 1);
}

function countReviewComments(
	row: CodeTreeRow,
	commentsByPath: Map<string, WorkspacePrReviewComment[]>,
): number {
	if (row.kind === "file") return commentsByPath.get(row.path)?.length ?? 0;
	if (commentsByPath.size === 0) return 0;
	const prefix = `${row.path}/`;
	let count = 0;
	for (const [path, comments] of commentsByPath) {
		if (path.startsWith(prefix)) count += comments.length;
	}
	return count;
}

/**
 * The worktree's files as a folder tree: new files show up before they are
 * staged, changed files carry their git status, the open file is revealed,
 * and the arrow keys move through it. The Inspector shows it with its own
 * header; the code column next to the sidebar brings its own chrome.
 */
export function CodeFilesTree({
	workspaceRoot,
	selectedPath,
	reviewCommentsByPath = EMPTY_REVIEW_COMMENTS,
	showHeader = true,
	onOpenFile,
	onOpenQuickOpen,
}: {
	workspaceRoot: string | null;
	selectedPath: string | null;
	reviewCommentsByPath?: Map<string, WorkspacePrReviewComment[]>;
	showHeader?: boolean;
	onOpenFile: (input: { path: string; name: string }) => void;
	onOpenQuickOpen: () => void;
}) {
	const { t } = useTranslation("common");
	const quickOpenShortcutKeys = getQuickOpenShortcutKeys();
	const root = workspaceRoot?.trim() ?? "";
	const filesQuery = useQuery({
		queryKey: ["inspectorCodeFiles", root],
		queryFn: async () => {
			const result = await listGitTrackedFiles({ workspaceRoot: root });
			return result.paths;
		},
		enabled: Boolean(root),
		staleTime: 30_000,
		refetchOnWindowFocus: true,
	});
	const gitStatusQuery = useWorkspaceGitStatus(root || null);
	const staged = gitStatusQuery.data?.staged ?? EMPTY_CHANGES;
	const unstaged = gitStatusQuery.data?.unstaged ?? EMPTY_CHANGES;
	const gitStatus = useMemo(() => codeTreeGitStatus(staged, unstaged), [staged, unstaged]);

	// Git status already polls; when it sees a file appear or go away, the
	// list is stale (an agent created or deleted something).
	const fileSet = useMemo(() => fileSetSignature(staged, unstaged), [staged, unstaged]);
	const lastFileSetRef = useRef<string | null>(null);
	const refetchFiles = filesQuery.refetch;
	useEffect(() => {
		if (!gitStatusQuery.data) return;
		if (lastFileSetRef.current !== null && lastFileSetRef.current !== fileSet) {
			void refetchFiles();
		}
		lastFileSetRef.current = fileSet;
	}, [fileSet, gitStatusQuery.data, refetchFiles]);

	const paths = filesQuery.data ?? EMPTY_CODE_FILE_PATHS;
	const tree = useMemo(() => buildCodeTree(paths), [paths]);
	const [expanded, setExpanded] = useState<Set<string>>(
		() => new Set(codeTreeExpansionCache.get(root) ?? []),
	);
	useEffect(() => {
		setExpanded(new Set(codeTreeExpansionCache.get(root) ?? []));
	}, [root]);
	const updateExpanded = useCallback(
		(update: (previous: Set<string>) => Set<string>) => {
			setExpanded((previous) => {
				const next = update(previous);
				if (next === previous) return previous;
				codeTreeExpansionCache.set(root, next);
				return next;
			});
		},
		[root],
	);
	const setFolderOpen = useCallback(
		(path: string, open: boolean) => {
			updateExpanded((previous) => {
				if (previous.has(path) === open) return previous;
				const next = new Set(previous);
				if (open) next.add(path);
				else next.delete(path);
				return next;
			});
		},
		[updateExpanded],
	);

	const rows = useMemo(() => visibleCodeTreeRows(tree, expanded), [tree, expanded]);
	const listRef = useRef<HTMLDivElement | null>(null);

	// Reveal the open file: open the folders above it and bring it into view.
	useEffect(() => {
		if (!selectedPath) return;
		updateExpanded((previous) => {
			const missing = ancestorFolderPaths(selectedPath).filter((path) => !previous.has(path));
			if (missing.length === 0) return previous;
			return new Set([...previous, ...missing]);
		});
	}, [selectedPath, updateExpanded]);
	// Scroll once per newly opened file (its row may only exist after the
	// folders above it open), never again while the user browses folders.
	const revealedPathRef = useRef<string | null>(null);
	useEffect(() => {
		if (!selectedPath || revealedPathRef.current === selectedPath) return;
		const row = listRef.current?.querySelector<HTMLElement>(
			`[data-tree-path="${CSS.escape(selectedPath)}"]`,
		);
		if (!row) return;
		row.scrollIntoView({ block: "nearest" });
		revealedPathRef.current = selectedPath;
	}, [rows, selectedPath]);

	const focusRow = (index: number) => {
		const items = listRef.current?.querySelectorAll<HTMLElement>("[data-tree-path]");
		if (!items || items.length === 0) return;
		items[Math.max(0, Math.min(items.length - 1, index))]?.focus();
	};
	const handleKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
		const path = (event.target as HTMLElement).dataset.treePath;
		if (!path) return;
		const index = rows.findIndex((row) => row.path === path);
		const row = rows[index];
		if (!row) return;
		switch (event.key) {
			case "ArrowDown":
				focusRow(index + 1);
				break;
			case "ArrowUp":
				focusRow(index - 1);
				break;
			case "Home":
				focusRow(0);
				break;
			case "End":
				focusRow(rows.length - 1);
				break;
			case "ArrowRight":
				if (row.kind !== "folder") return;
				if (!row.expanded) setFolderOpen(row.path, true);
				else focusRow(index + 1);
				break;
			case "ArrowLeft":
				if (row.kind === "folder" && row.expanded) {
					setFolderOpen(row.path, false);
				} else if (row.parentPath) {
					focusRow(rows.findIndex((candidate) => candidate.path === row.parentPath));
				}
				break;
			default:
				return;
		}
		event.preventDefault();
	};

	return (
		<section className="flex min-h-0 flex-1 flex-col overflow-hidden border-b border-border/60">
			{showHeader ? (
				<div className="flex h-11 shrink-0 items-center justify-between gap-3 border-b border-border/50 px-3">
					<div className="min-w-0">
						<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							{t("inspector.codeSection.kicker")}
						</p>
						<p className="truncate text-[13px] font-medium leading-tight text-foreground">
							{t("inspector.codeSection.title")}
						</p>
					</div>
					<div className="flex shrink-0 items-center gap-1.5">
						<Badge variant="outline" className="h-6 px-2 text-[10px] font-normal">
							{t("inspector.codeSection.filesCount", { count: paths.length })}
						</Badge>
						<Tooltip>
							<TooltipTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="sm"
									className="h-7 cursor-pointer gap-1.5 px-2 text-muted-foreground hover:text-foreground"
									onClick={onOpenQuickOpen}
									aria-label={t("inspector.codeSection.quickOpen")}
								>
									<Search className="size-3.5" strokeWidth={1.8} />
									<InlineShortcutDisplay keys={quickOpenShortcutKeys} />
								</Button>
							</TooltipTrigger>
							<TooltipContent side="bottom" className="gap-2">
								<span>{t("inspector.codeSection.quickOpen")}</span>
								<InlineShortcutDisplay keys={quickOpenShortcutKeys} />
							</TooltipContent>
						</Tooltip>
					</div>
				</div>
			) : null}
			<div
				data-inspector-scroll-key="code-files"
				className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-2 py-2"
			>
				{!root ? (
					<p className="px-2 py-3 text-[12px] text-muted-foreground">
						{t("inspector.codeSection.unavailable")}
					</p>
				) : filesQuery.isPending ? (
					<div className="flex items-center gap-2 px-2 py-3 text-[12px] text-muted-foreground">
						<Loader2 className="size-3.5 animate-spin" strokeWidth={1.8} />
						<span>{t("inspector.codeSection.loading")}</span>
					</div>
				) : filesQuery.isError ? (
					<p className="px-2 py-3 text-[12px] text-destructive">
						{(filesQuery.error as Error | null)?.message ??
							t("inspector.codeSection.error")}
					</p>
				) : paths.length === 0 ? (
					<p className="px-2 py-3 text-[12px] text-muted-foreground">
						{t("inspector.codeSection.empty")}
					</p>
				) : (
					<div
						ref={listRef}
						role="tree"
						aria-label={t("codeColumn.files")}
						onKeyDown={handleKeyDown}
					>
						{rows.map((row) => (
							<CodeTreeRowButton
								key={row.path}
								row={row}
								selected={row.kind === "file" && row.path === selectedPath}
								gitStatus={row.kind === "file" ? gitStatus.byPath.get(row.path) : undefined}
								folderChanged={row.kind === "folder" && gitStatus.changedFolders.has(row.path)}
								reviewCount={countReviewComments(row, reviewCommentsByPath)}
								onToggleFolder={() => row.kind === "folder" && setFolderOpen(row.path, !row.expanded)}
								onOpenFile={() => onOpenFile({ path: row.path, name: basename(row.path) })}
							/>
						))}
					</div>
				)}
			</div>
		</section>
	);
}

function CodeTreeRowButton({
	row,
	selected,
	gitStatus,
	folderChanged,
	reviewCount,
	onToggleFolder,
	onOpenFile,
}: {
	row: CodeTreeRow;
	selected: boolean;
	gitStatus: CodeTreeGitStatus | undefined;
	folderChanged: boolean;
	reviewCount: number;
	onToggleFolder: () => void;
	onOpenFile: () => void;
}) {
	const isFolder = row.kind === "folder";
	// Folder clicks keep the editor's focus; keyboard focus still lands on rows.
	const keepEditorFocus = (event: ReactMouseEvent | ReactPointerEvent) => {
		if (isFolder) event.preventDefault();
	};
	return (
		<button
			type="button"
			role="treeitem"
			data-tree-path={row.path}
			aria-expanded={isFolder ? row.expanded : undefined}
			aria-selected={selected}
			aria-level={row.depth + 1}
			title={row.path}
			onPointerDown={keepEditorFocus}
			onMouseDown={keepEditorFocus}
			onClick={isFolder ? onToggleFolder : onOpenFile}
			className={cn(
				"flex h-6 w-full cursor-pointer items-center gap-1.5 rounded-sm pr-2 text-left outline-none transition-colors focus-visible:bg-accent/60 focus-visible:ring-1 focus-visible:ring-ring",
				selected
					? "bg-muted/70 text-foreground"
					: "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
			)}
			style={{ paddingLeft: row.depth * 12 + (isFolder ? 6 : 20) }}
		>
			{isFolder ? (
				<ChevronRight
					className={cn("size-3 shrink-0 transition-transform", row.expanded && "rotate-90")}
					strokeWidth={2}
				/>
			) : null}
			<img
				src={
					isFolder
						? getMaterialFolderIcon(basename(row.path), row.expanded || undefined)
						: getMaterialFileIcon(row.label)
				}
				alt=""
				className="size-3.5 shrink-0"
			/>
			<span
				className={cn(
					"min-w-0 truncate text-[12px] leading-6",
					gitStatus && GIT_TONE_CLASS[gitStatus.tone],
				)}
			>
				{row.label}
			</span>
			<span className="ml-auto flex shrink-0 items-center gap-1.5">
				{reviewCount > 0 ? (
					<span className="inline-flex h-4 items-center gap-0.5 rounded-full bg-primary/10 px-1 text-[9.5px] font-semibold text-primary">
						<MessageSquare className="size-2.5" strokeWidth={2} />
						{reviewCount}
					</span>
				) : null}
				{gitStatus ? (
					<span
						className={cn(
							"w-2.5 text-center font-mono text-[10px] font-semibold",
							GIT_TONE_CLASS[gitStatus.tone],
						)}
					>
						{gitStatus.letter}
					</span>
				) : folderChanged ? (
					<span className="size-1.5 rounded-full bg-amber-500/80" aria-hidden />
				) : null}
			</span>
		</button>
	);
}
