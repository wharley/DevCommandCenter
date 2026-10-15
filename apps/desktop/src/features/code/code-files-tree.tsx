import type { WorkspacePrReviewComment } from "@dcc/contracts";
import { useQuery } from "@tanstack/react-query";
import { getMaterialFileIcon, getMaterialFolderIcon } from "file-extension-icon-js";
import { ChevronRight, Loader2, MessageSquare, Search } from "lucide-react";
import {
	useCallback,
	useEffect,
	useMemo,
	useState,
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

const EMPTY_CODE_FILE_PATHS: string[] = [];
const EMPTY_REVIEW_COMMENTS = new Map<string, WorkspacePrReviewComment[]>();
/** Expanded folders per worktree, kept across remounts of the tree. */
const codeTreeExpansionCache = new Map<string, Set<string>>();

function basename(path: string): string {
	const slash = path.lastIndexOf("/");
	return slash === -1 ? path : path.slice(slash + 1);
}

type CodeTreeNode = {
	name: string;
	path: string;
	children: Map<string, CodeTreeNode>;
	file?: string;
};

function buildCodeTree(paths: string[]): CodeTreeNode {
	const root: CodeTreeNode = { name: "", path: "", children: new Map() };

	for (const filePath of paths) {
		const parts = filePath.split("/").filter(Boolean);
		if (parts.length === 0) {
			continue;
		}
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
			path: filePath,
			children: new Map(),
			file: filePath,
		});
	}

	return root;
}

function collectInitialCodeFolderPaths(node: CodeTreeNode): string[] {
	const paths: string[] = [];
	for (const child of node.children.values()) {
		if (child.children.size > 0 && !child.file) {
			paths.push(child.path);
		}
	}
	return paths;
}

function sameStringSet(left: Set<string>, right: Set<string>): boolean {
	if (left.size !== right.size) {
		return false;
	}
	for (const item of left) {
		if (!right.has(item)) {
			return false;
		}
	}
	return true;
}

function countReviewCommentsForNode(
	node: CodeTreeNode,
	commentsByPath: Map<string, WorkspacePrReviewComment[]>,
): number {
	if (node.file) {
		return commentsByPath.get(node.file)?.length ?? 0;
	}
	let count = 0;
	for (const child of node.children.values()) {
		count += countReviewCommentsForNode(child, commentsByPath);
	}
	return count;
}

/**
 * The worktree's tracked files as a folder tree. The Inspector shows it with
 * its own header; the code column next to the sidebar brings its own chrome.
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
		refetchOnWindowFocus: false,
	});
	const paths = filesQuery.data ?? EMPTY_CODE_FILE_PATHS;
	const tree = useMemo(() => buildCodeTree(paths), [paths]);
	const [expanded, setExpanded] = useState<Set<string>>(
		() => new Set(codeTreeExpansionCache.get(root) ?? []),
	);

	useEffect(() => {
		const cached = codeTreeExpansionCache.get(root);
		const next = cached
			? new Set(cached)
			: new Set(collectInitialCodeFolderPaths(tree));
		codeTreeExpansionCache.set(root, next);
		setExpanded((current) => (sameStringSet(current, next) ? current : next));
	}, [root, tree]);

	const toggle = useCallback((path: string) => {
		setExpanded((previous) => {
			const next = new Set(previous);
			if (next.has(path)) {
				next.delete(path);
			} else {
				next.add(path);
			}
			codeTreeExpansionCache.set(root, next);
			return next;
		});
	}, [root]);

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
					<CodeTreeNodeList
						nodes={tree.children}
						expanded={expanded}
						onToggle={toggle}
						depth={0}
						selectedPath={selectedPath}
						reviewCommentsByPath={reviewCommentsByPath}
						onOpenFile={onOpenFile}
					/>
				)}
			</div>
		</section>
	);
}

function CodeTreeNodeList({
	nodes,
	expanded,
	onToggle,
	depth,
	selectedPath,
	reviewCommentsByPath,
	onOpenFile,
}: {
	nodes: Map<string, CodeTreeNode>;
	expanded: Set<string>;
	onToggle: (path: string) => void;
	depth: number;
	selectedPath: string | null;
	reviewCommentsByPath: Map<string, WorkspacePrReviewComment[]>;
	onOpenFile: (input: { path: string; name: string }) => void;
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
	const keepEditorFocusOnMouseDown = (event: ReactMouseEvent<HTMLButtonElement>) => {
		event.preventDefault();
	};
	const keepEditorFocusOnPointerDown = (
		event: ReactPointerEvent<HTMLButtonElement>,
	) => {
		event.preventDefault();
	};

	return (
		<div role={depth === 0 ? "tree" : "group"}>
			{sorted.map((node) => {
				const isFolder = node.children.size > 0 && !node.file;
				if (isFolder) {
					const isOpen = expanded.has(node.path);
					const reviewCount = countReviewCommentsForNode(node, reviewCommentsByPath);
					return (
						<div key={node.path}>
							<button
								type="button"
								role="treeitem"
								aria-expanded={isOpen}
								onPointerDown={keepEditorFocusOnPointerDown}
								onMouseDown={keepEditorFocusOnMouseDown}
								onClick={() => onToggle(node.path)}
								className="flex h-6 w-full cursor-pointer items-center gap-1 rounded-sm pr-2 text-left text-[11.5px] text-muted-foreground outline-none transition-colors hover:bg-accent/60 focus-visible:bg-accent/60 focus-visible:ring-1 focus-visible:ring-ring"
								style={{ paddingLeft: pad }}
							>
								<ChevronRight
									className={cn("size-3 shrink-0 transition-transform", isOpen && "rotate-90")}
									strokeWidth={2}
								/>
								<img
									src={getMaterialFolderIcon(node.name, isOpen || undefined)}
									alt=""
									className="size-3.5 shrink-0"
								/>
								<span className="min-w-0 truncate text-[12px] leading-6">{node.name}</span>
								{reviewCount > 0 ? (
									<span className="ml-auto inline-flex h-4 shrink-0 items-center gap-0.5 rounded-full bg-primary/10 px-1 text-[9.5px] font-semibold text-primary">
										<MessageSquare className="size-2.5" strokeWidth={2} />
										{reviewCount}
									</span>
								) : null}
							</button>
							{isOpen ? (
								<CodeTreeNodeList
									nodes={node.children}
									expanded={expanded}
									onToggle={onToggle}
									depth={depth + 1}
									selectedPath={selectedPath}
									reviewCommentsByPath={reviewCommentsByPath}
									onOpenFile={onOpenFile}
								/>
							) : null}
						</div>
					);
				}

				const filePath = node.file;
				if (!filePath) {
					return null;
				}
				const selected = selectedPath === filePath;
				const reviewCount = reviewCommentsByPath.get(filePath)?.length ?? 0;
				return (
					<button
						key={filePath}
						type="button"
						role="treeitem"
						title={filePath}
						onClick={() => onOpenFile({ path: filePath, name: basename(filePath) })}
						className={cn(
							"flex h-6 w-full cursor-pointer items-center gap-1.5 rounded-sm pr-2 text-left text-[11.5px] outline-none transition-colors focus-visible:ring-1 focus-visible:ring-ring",
							selected
								? "bg-muted/70 text-foreground"
								: "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
						)}
						style={{ paddingLeft: depth * 12 + 22 }}
					>
						<img
							src={getMaterialFileIcon(node.name)}
							alt=""
							className="size-3.5 shrink-0"
						/>
						<span className="min-w-0 truncate text-[12px] leading-6">{node.name}</span>
						{reviewCount > 0 ? (
							<span className="ml-auto inline-flex h-4 shrink-0 items-center gap-0.5 rounded-full bg-primary/10 px-1 text-[9.5px] font-semibold text-primary">
								<MessageSquare className="size-2.5" strokeWidth={2} />
								{reviewCount}
							</span>
						) : null}
					</button>
				);
			})}
		</div>
	);
}

