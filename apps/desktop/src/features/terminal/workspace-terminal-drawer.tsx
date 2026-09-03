import {
	ChevronDown,
	Eraser,
	ExternalLink,
	Globe2,
	GripHorizontal,
	Maximize2,
	MoreHorizontal,
	Minimize2,
	PanelBottomClose,
	Pencil,
	Plus,
	RefreshCcw,
	ShieldCheck,
	Sparkles,
	Square,
	TriangleAlert,
	X,
} from "lucide-react";
import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
	type FormEvent,
	type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { useTranslation } from "react-i18next";
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
import { Input } from "@/components/ui/input";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { suspendTerminalFit } from "@/components/terminal-output";
import { TerminalPanel, type TerminalPanelHandle } from "./terminal-panel";
import {
	addTerminal,
	ensureTerminal,
	getTerminalRuntimeId,
	MAX_TERMINAL_TABS,
	removeTerminal,
	renameTerminal,
	setActiveTerminal,
	useProjectTerminals,
} from "./terminal-tabs-store";
import {
	getTerminalSnapshot,
	getTerminalContextExcerpt,
	interruptTerminal,
	restartTerminal,
	subscribeTerminalStore,
	type TerminalSnapshot,
} from "./terminal-store";
import type { TerminalScopeKind, TerminalScopeTarget } from "./terminal-scope";
import { cn } from "@/lib/utils";
import { getTerminalTabNavigationTarget } from "./terminal-tab-navigation";
import {
	limitTerminalSelection,
	resolveTerminalAgentContent,
} from "./terminal-selection";
import { openTerminalAtPath } from "@/lib/shell-api";
import { formatDevServerAddress } from "./dev-server-detection";

const HEIGHT_STORAGE_KEY = "dcc-workbench-terminal-dock-height-v1";
const DEFAULT_HEIGHT_PX = 340;
const MIN_HEIGHT_PX = 220;
const COMPACT_MIN_HEIGHT_PX = 160;
export const MAX_AGENT_SELECTION_CHARS = 16_000;
/** Keep the conversation usable while the terminal rests below it. */
const MIN_CHAT_HEIGHT_PX = 360;

function maxDockHeight(dock: HTMLDivElement | null): number {
	const parentHeight =
		dock?.parentElement?.getBoundingClientRect().height ?? window.innerHeight;
	return Math.max(COMPACT_MIN_HEIGHT_PX, parentHeight - MIN_CHAT_HEIGHT_PX);
}

function clampHeight(px: number, maxPx: number) {
	const safe = Number.isFinite(px) ? px : DEFAULT_HEIGHT_PX;
	const upper = Math.max(COMPACT_MIN_HEIGHT_PX, maxPx);
	const lower = Math.min(MIN_HEIGHT_PX, upper);
	return Math.round(Math.min(Math.max(safe, lower), upper));
}

function statusDotClass(status: TerminalSnapshot["activityStatus"] | "ready") {
	if (status === "running") return "bg-sky-500";
	if (status === "waiting") return "bg-amber-400";
	if (status === "error") return "bg-destructive";
	if (status === "exited") return "bg-muted-foreground";
	return "bg-muted-foreground/45";
}

function terminalDisplayTitle(title: string, snapshot: TerminalSnapshot | null) {
	if (/^Terminal \d+$/u.test(title) && snapshot?.activityLabel) {
		return snapshot.activityLabel;
	}
	return title;
}

export type TerminalAgentContext = {
	workspaceId: string;
	sessionId: string | null;
	title: string;
	projectLabel: string;
	scopeLabel: string;
	branchLabel: string | null;
	cwd: string;
	content: string;
	selectionOnly: boolean;
};

export type WorkspaceTerminalDrawerProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	/** Expanded = terminal takes over the full workbench (chat hidden). */
	expanded: boolean;
	onExpandedChange: (expanded: boolean) => void;
	/** Terminals are scoped by project root or by worktree/mission. */
	scopeKey: string;
	scopeLabel: string;
	/** CWD for this terminal scope. */
	cwd: string | null;
	scopes: TerminalScopeTarget[];
	activeScopeKind: TerminalScopeKind;
	onScopeChange: (scope: TerminalScopeKind) => void;
	workspaceName: string;
	workspaceId: string;
	workspaceBranch: string;
	providerLabel: string | null;
	sessionState: string;
	sessionId: string | null;
	onSendToAgent?: (context: TerminalAgentContext) => void;
	onOpenDetectedServer?: (url: string) => void;
	className?: string;
};

/**
 * Bottom dock — general-purpose terminals scoped to a project or worktree.
 *
 * Default: a resizable vertical split so the wide terminal surface does not
 * compete with the Inspector for horizontal space.
 * Expanded: promotes to a full-bleed surface (chat hidden) for terminal-only focus.
 */
export function WorkspaceTerminalDrawer({
	open,
	onOpenChange,
	expanded,
	onExpandedChange,
	scopeKey,
	scopeLabel,
	cwd,
	scopes,
	activeScopeKind,
	onScopeChange,
	workspaceName,
	workspaceId,
	workspaceBranch,
	providerLabel,
	sessionState,
	sessionId,
	onSendToAgent,
	onOpenDetectedServer,
	className,
}: WorkspaceTerminalDrawerProps) {
	const { t } = useTranslation("common");
	const dockRef = useRef<HTMLDivElement | null>(null);
	const dragRef = useRef<{ startY: number; startH: number } | null>(null);
	const releaseFitRef = useRef<(() => void) | null>(null);
	const terminalTabRefs = useRef(new Map<string, HTMLButtonElement>());
	const terminalPanelRef = useRef<TerminalPanelHandle | null>(null);
	const terminalSelectionRef = useRef("");
	const heightRef = useRef(DEFAULT_HEIGHT_PX);
	const [heightPx, setHeightPx] = useState(DEFAULT_HEIGHT_PX);
	const [terminalStatusVersion, setTerminalStatusVersion] = useState(0);
	const [renamingTab, setRenamingTab] = useState<{ id: string; title: string } | null>(null);
	const [closingTab, setClosingTab] = useState<{ id: string; title: string } | null>(null);
	const [tabTitleDraft, setTabTitleDraft] = useState("");
	const [hasTerminalSelection, setHasTerminalSelection] = useState(false);

	const { tabs, activeId } = useProjectTerminals(scopeKey);
	const activeTab = tabs.find((tab) => tab.id === activeId) ?? tabs[0] ?? null;
	const atCap = tabs.length >= MAX_TERMINAL_TABS;
	const terminalSnapshots = useMemo(
		() =>
			new Map<string, TerminalSnapshot | null>(
				tabs.map((tab): [string, TerminalSnapshot | null] => [
					tab.id,
					getTerminalSnapshot(getTerminalRuntimeId(scopeKey, tab.id)),
				]),
			),
		[scopeKey, tabs, terminalStatusVersion],
	);
	const detectedDevServer = useMemo(
		() =>
			[...terminalSnapshots.values()]
				.flatMap((terminal) => terminal?.detectedDevServers ?? [])
				.sort((left, right) => right.detectedAt - left.detectedAt)[0] ?? null,
		[terminalSnapshots],
	);

	useEffect(
		() => subscribeTerminalStore(() => setTerminalStatusVersion((value) => value + 1)),
		[],
	);

	// Make sure an open dock always has at least one terminal to show.
	useEffect(() => {
		if (open && cwd) {
			ensureTerminal(scopeKey);
		}
	}, [cwd, open, scopeKey]);

	useEffect(() => {
		heightRef.current = heightPx;
	}, [heightPx]);

	useLayoutEffect(() => {
		try {
			const raw = localStorage.getItem(HEIGHT_STORAGE_KEY);
			const n = raw ? Number.parseInt(raw, 10) : DEFAULT_HEIGHT_PX;
			if (Number.isFinite(n)) {
				setHeightPx(clampHeight(n, maxDockHeight(dockRef.current)));
			}
		} catch {
			/* ignore */
		}
	}, []);

	useEffect(() => {
		if (!open) {
			return;
		}

		const onKey = (event: KeyboardEvent) => {
			if (event.key !== "Escape") {
				return;
			}
			// Esc steps back one level: collapse first, then close.
			event.preventDefault();
			if (expanded) {
				onExpandedChange(false);
			} else {
				onOpenChange(false);
			}
		};

		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, [open, expanded, onExpandedChange, onOpenChange]);

	const beginResize = useCallback(
		(clientY: number) => {
			dragRef.current = { startY: clientY, startH: heightPx };
			// Hold xterm refits until the drag ends, then fit once — avoids
			// flooding the PTY with resize events on every mousemove frame.
			releaseFitRef.current = suspendTerminalFit();
		},
		[heightPx],
	);

	const applyResizeMove = useCallback((clientY: number) => {
		const drag = dragRef.current;
		if (!drag) {
			return;
		}
		// Dock sits at the bottom: dragging the handle up grows it.
		const delta = drag.startY - clientY;
		const next = clampHeight(drag.startH + delta, maxDockHeight(dockRef.current));
		setHeightPx(next);
	}, []);

	const endResize = useCallback(() => {
		dragRef.current = null;
		releaseFitRef.current?.();
		releaseFitRef.current = null;
		try {
			localStorage.setItem(HEIGHT_STORAGE_KEY, String(heightRef.current));
		} catch {
			/* ignore */
		}
	}, []);

	const handleTerminalTabKeyDown = useCallback(
		(event: ReactKeyboardEvent<HTMLButtonElement>, currentId: string) => {
			const targetId = getTerminalTabNavigationTarget(
				tabs.map((tab) => tab.id),
				currentId,
				event.key,
			);
			if (!targetId) return;
			event.preventDefault();
			setActiveTerminal(scopeKey, targetId);
			requestAnimationFrame(() => terminalTabRefs.current.get(targetId)?.focus());
		},
		[scopeKey, tabs],
	);

	const openRenameTab = useCallback((tab: { id: string; title: string }) => {
		setRenamingTab(tab);
		setTabTitleDraft(tab.title);
	}, []);

	const submitRenameTab = useCallback(
		(event: FormEvent<HTMLFormElement>) => {
			event.preventDefault();
			if (!renamingTab) return;
			if (renameTerminal(scopeKey, renamingTab.id, tabTitleDraft)) {
				setRenamingTab(null);
			}
		},
		[renamingTab, scopeKey, tabTitleDraft],
	);

	const activeRuntimeId = activeTab
		? getTerminalRuntimeId(scopeKey, activeTab.id)
		: null;
	const activeSnapshot = activeTab ? terminalSnapshots.get(activeTab.id) ?? null : null;
	const closingSnapshot = closingTab
		? terminalSnapshots.get(closingTab.id) ?? null
		: null;
	const activeScope = scopes.find((scope) => scope.kind === activeScopeKind) ?? scopes[0];
	const activeProcess =
		activeSnapshot?.activityStatus === "running" ||
		activeSnapshot?.activityStatus === "waiting";

	const handleTerminalSelectionChange = useCallback((selection: string) => {
		const boundedSelection = limitTerminalSelection(
			selection,
			MAX_AGENT_SELECTION_CHARS,
		);
		terminalSelectionRef.current = boundedSelection;
		setHasTerminalSelection(boundedSelection.length > 0);
	}, []);

	const clearTerminalSelection = useCallback(() => {
		terminalSelectionRef.current = "";
		setHasTerminalSelection(false);
	}, []);

	useEffect(() => {
		clearTerminalSelection();
	}, [activeRuntimeId, clearTerminalSelection]);

	useEffect(() => {
		if (!open) {
			clearTerminalSelection();
		}
	}, [clearTerminalSelection, open]);

	const handleSendToAgent = useCallback(() => {
		if (!activeTab || !activeRuntimeId || !cwd || !activeScope || !onSendToAgent) return;
		const { content, selectionOnly } = resolveTerminalAgentContent(
			terminalSelectionRef.current || terminalPanelRef.current?.getSelection() || "",
			getTerminalContextExcerpt(activeRuntimeId),
			MAX_AGENT_SELECTION_CHARS,
		);
		if (!content) return;
		onSendToAgent({
			workspaceId,
			sessionId,
			title: activeTab.title,
			projectLabel: activeScope.projectLabel,
			scopeLabel: activeScope.label,
			branchLabel: activeScope.branchLabel,
			cwd,
			content,
			selectionOnly,
		});
		clearTerminalSelection();
	}, [
		activeRuntimeId,
		activeScope,
		activeTab,
		clearTerminalSelection,
		cwd,
		onSendToAgent,
		sessionId,
		workspaceId,
	]);

	const handleRestart = useCallback(async () => {
		if (!activeRuntimeId || !activeTab || !cwd) return;
		clearTerminalSelection();
		terminalPanelRef.current?.clear();
		await restartTerminal(activeRuntimeId, cwd, {
			title: activeTab.title,
			workspaceName,
			workspaceBranch,
			providerLabel,
			sessionState,
			sessionId,
		});
		requestAnimationFrame(() => terminalPanelRef.current?.focus());
	}, [
		activeRuntimeId,
		activeTab,
		cwd,
		clearTerminalSelection,
		providerLabel,
		sessionId,
		sessionState,
		workspaceBranch,
		workspaceName,
	]);

	useEffect(() => {
		const onMove = (event: MouseEvent) => {
			if (dragRef.current) {
				applyResizeMove(event.clientY);
			}
		};
		const onUp = () => {
			if (dragRef.current) {
				endResize();
			}
		};

		window.addEventListener("mousemove", onMove);
		window.addEventListener("mouseup", onUp);
		return () => {
			window.removeEventListener("mousemove", onMove);
			window.removeEventListener("mouseup", onUp);
		};
	}, [applyResizeMove, endResize]);

	// Keep height within bounds when the window shrinks.
	useEffect(() => {
		const onResize = () => {
			setHeightPx((current) => clampHeight(current, maxDockHeight(dockRef.current)));
		};
		window.addEventListener("resize", onResize);
		return () => window.removeEventListener("resize", onResize);
	}, []);

	if (!open) {
		return null;
	}

	return (
		<div
			ref={dockRef}
			className={cn(
				"dcc-workbench-terminal-dock flex min-h-0 min-w-0 flex-col overflow-hidden bg-card text-card-foreground",
				expanded ? "flex-1" : "shrink-0 border-t border-border",
				className,
			)}
			style={expanded ? undefined : { height: heightPx }}
			role="region"
			aria-label={t("terminalDock.ariaLabel")}
		>
			{!expanded ? (
				<Tooltip>
					<TooltipTrigger asChild>
						<button
							type="button"
							className={cn(
								"dcc-workbench-terminal-dock__resize group flex h-1.5 w-full shrink-0 cursor-ns-resize items-center justify-center border-b border-border/60 bg-muted/20",
								"hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
							)}
							aria-label={t("terminalDock.resize")}
							onMouseDown={(event) => {
								event.preventDefault();
								beginResize(event.clientY);
							}}
						>
							<GripHorizontal className="size-3 text-muted-foreground/0 transition-colors group-hover:text-muted-foreground" />
						</button>
					</TooltipTrigger>
					<TooltipContent side="top">{t("terminalDock.resizeHint")}</TooltipContent>
				</Tooltip>
			) : null}

			<div className="flex min-h-0 min-w-0 flex-1 flex-col">
				<div className="flex h-[var(--dcc-terminal-toolbar-height)] shrink-0 items-center gap-2 border-b border-border/60 px-2">
					<div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
						<DropdownMenu>
							<DropdownMenuTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="sm"
									className="mr-1 h-7 shrink-0 gap-1 rounded-md bg-muted/30 px-2 text-[var(--dcc-daily-meta-size)] font-medium text-muted-foreground"
									aria-label={t("terminalDock.scopeSelector", { scope: scopeLabel })}
								>
									{activeScope?.protected ? (
										<ShieldCheck className="size-3.5 text-emerald-500" />
									) : (
										<TriangleAlert className="size-3.5 text-amber-500" />
									)}
									<span className="max-w-[18rem] truncate">
										{activeScope
											? `${activeScope.label} · ${activeScope.projectLabel}${activeScope.branchLabel ? ` · ${activeScope.branchLabel}` : ""}`
											: scopeLabel}
									</span>
									<ChevronDown className="size-3 opacity-50" />
								</Button>
							</DropdownMenuTrigger>
							<DropdownMenuContent align="start" className="w-56">
								{scopes.map((scope) => (
									<DropdownMenuItem
										key={scope.kind}
										disabled={!scope.cwd}
										className="flex items-center justify-between gap-3"
										onClick={() => onScopeChange(scope.kind)}
									>
										<span className="flex min-w-0 items-start gap-2">
											{scope.protected ? (
												<ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-emerald-500" />
											) : (
												<TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-amber-500" />
											)}
											<span className="min-w-0">
												<span className="block truncate font-medium">
													{scope.label} · {scope.projectLabel}
												</span>
												<span className="block truncate text-[11px] text-muted-foreground">
													{scope.branchLabel ?? scope.cwd ?? scope.disabledReason}
												</span>
											</span>
										</span>
										{scope.kind === activeScopeKind ? <span>✓</span> : null}
									</DropdownMenuItem>
								))}
							</DropdownMenuContent>
						</DropdownMenu>
						<div
							role="tablist"
							aria-label={t("terminalDock.tabsAriaLabel")}
							className="flex min-w-0 items-center gap-1"
						>
							{tabs.map((tab) => {
								const isActive = tab.id === activeTab?.id;
								const snapshot = terminalSnapshots.get(tab.id) ?? null;
								const status = snapshot?.activityStatus ?? "ready";
								const displayTitle = terminalDisplayTitle(tab.title, snapshot);
								return (
								<div
									key={tab.id}
									role="presentation"
									className={cn(
										"group/tab flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-[var(--dcc-daily-meta-size)] transition-colors",
										isActive
											? "bg-background text-foreground shadow-[var(--dcc-elevation-1)]"
											: "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
									)}
								>
									<button
										type="button"
										ref={(node) => {
											if (node) terminalTabRefs.current.set(tab.id, node);
											else terminalTabRefs.current.delete(tab.id);
										}}
										id={`terminal-tab-${tab.id}`}
										role="tab"
										aria-selected={isActive}
										aria-controls={`terminal-panel-${tab.id}`}
										tabIndex={isActive ? 0 : -1}
										className="max-w-[9rem] truncate"
										title={
											snapshot?.activityLabel
												? `${tab.title} · ${snapshot.activityLabel}`
												: tab.title
										}
										onClick={() => setActiveTerminal(scopeKey, tab.id)}
										onKeyDown={(event) => handleTerminalTabKeyDown(event, tab.id)}
									>
										<span className="flex items-center gap-1.5">
											<span
												className={cn(
													"size-1.5 rounded-full",
													status === "waiting" && "animate-pulse",
													statusDotClass(status),
												)}
											/>
											<span className="truncate">{displayTitle}</span>
										</span>
									</button>
									<button
										type="button"
										aria-label={t("terminalDock.renameTab", { title: tab.title })}
										className="rounded p-0.5 text-muted-foreground/60 opacity-0 transition-opacity hover:bg-muted hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring group-hover/tab:opacity-100"
										onClick={() => openRenameTab(tab)}
									>
										<Pencil className="size-3" />
									</button>
									<button
										type="button"
										aria-label={t("terminalDock.closeTab", { title: tab.title })}
										className="rounded p-0.5 text-muted-foreground/60 opacity-0 transition-opacity hover:bg-muted hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring group-hover/tab:opacity-100"
										onClick={(event) => {
											event.stopPropagation();
											if (status === "running" || status === "waiting") {
												setClosingTab(tab);
											} else {
												removeTerminal(scopeKey, tab.id);
											}
										}}
									>
										<X className="size-3" />
									</button>
								</div>
								);
							})}
						</div>
						<Tooltip>
							<TooltipTrigger asChild>
								<Button
									type="button"
									size="sm"
									variant="ghost"
									className="h-7 shrink-0 px-1.5"
									aria-label={t("terminalDock.newTab")}
									disabled={atCap || !cwd}
									onClick={() => addTerminal(scopeKey)}
								>
									<Plus className="size-4" />
								</Button>
							</TooltipTrigger>
							<TooltipContent side="bottom">
								{atCap
									? t("terminalDock.maxTabs", { count: MAX_TERMINAL_TABS })
									: cwd
										? t("terminalDock.newScopedTab", { scope: scopeLabel })
										: t("terminalDock.noPath")}
							</TooltipContent>
						</Tooltip>
					</div>
					{detectedDevServer && onOpenDetectedServer ? (
						<Tooltip>
							<TooltipTrigger asChild>
								<Button
									type="button"
									size="sm"
									variant="outline"
									className="h-7 max-w-52 shrink-0 gap-1.5 px-2 text-xs"
									onClick={() => onOpenDetectedServer(detectedDevServer.url)}
								>
									<Globe2 className="size-3.5 shrink-0" />
									<span className="truncate">
										{t("terminalDock.openDevServer", {
											address: formatDevServerAddress(detectedDevServer.url),
										})}
									</span>
								</Button>
							</TooltipTrigger>
							<TooltipContent side="bottom">
								{t("terminalDock.openDevServerHint", {
									url: detectedDevServer.url,
								})}
							</TooltipContent>
						</Tooltip>
					) : null}
					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button
								type="button"
								size="sm"
								variant="ghost"
								className="h-8 shrink-0 px-2"
								aria-label={t("terminalDock.actions")}
							>
								<MoreHorizontal className="size-4" />
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end" className="w-56">
							<DropdownMenuItem
								disabled={!activeTab || !onSendToAgent}
								onClick={handleSendToAgent}
							>
								<Sparkles className="size-4" />
								{t("terminalDock.sendToAgent")}
							</DropdownMenuItem>
							<DropdownMenuItem
								disabled={!activeRuntimeId}
								onClick={() => {
									clearTerminalSelection();
									terminalPanelRef.current?.clear();
								}}
							>
								<Eraser className="size-4" />
								{t("terminalDock.clear")}
							</DropdownMenuItem>
							<DropdownMenuItem
								disabled={!activeRuntimeId || !activeProcess}
								onClick={() =>
									activeRuntimeId && void interruptTerminal(activeRuntimeId)
								}
							>
								<Square className="size-4" />
								{t("terminalDock.interrupt")}
							</DropdownMenuItem>
							<DropdownMenuItem
								disabled={!activeRuntimeId || !cwd || activeProcess}
								onClick={() => void handleRestart()}
							>
								<RefreshCcw className="size-4" />
								{t("terminalDock.restart")}
							</DropdownMenuItem>
							<DropdownMenuItem
								disabled={!cwd}
								onClick={() => cwd && void openTerminalAtPath(cwd)}
							>
								<ExternalLink className="size-4" />
								{t("terminalDock.openExternal")}
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								size="sm"
								variant="ghost"
								className="h-8 shrink-0 px-2"
								aria-label={
								expanded ? t("terminalDock.collapse") : t("terminalDock.expand")
							}
								onClick={() => onExpandedChange(!expanded)}
							>
								{expanded ? (
									<Minimize2 className="size-4" />
								) : (
									<Maximize2 className="size-4" />
								)}
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">
							{expanded ? t("terminalDock.collapseHint") : t("terminalDock.expand")}
						</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								size="sm"
								variant="outline"
								className="h-8 shrink-0 px-2"
								aria-label={t("terminalDock.hide")}
								onClick={() => onOpenChange(false)}
							>
								<PanelBottomClose className="size-4" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">
							{t("terminalDock.hideHint")}
						</TooltipContent>
					</Tooltip>
				</div>

				<div
					id={activeTab ? `terminal-panel-${activeTab.id}` : undefined}
					role="tabpanel"
					aria-labelledby={activeTab ? `terminal-tab-${activeTab.id}` : undefined}
					className="dcc-workbench-terminal-dock__panel relative flex min-h-0 flex-1 flex-col px-2 pb-2 pt-1"
				>
					{hasTerminalSelection && onSendToAgent ? (
						<Button
							type="button"
							size="sm"
							className="absolute right-4 top-3 z-10 h-8 gap-1.5 shadow-lg"
							onMouseDown={(event) => event.preventDefault()}
							onClick={handleSendToAgent}
						>
							<Sparkles className="size-3.5" aria-hidden="true" />
							{t("terminalDock.sendSelectionToAgent")}
						</Button>
					) : null}
					{activeTab ? (
						<TerminalPanel
							ref={terminalPanelRef}
							key={getTerminalRuntimeId(scopeKey, activeTab.id)}
							variant="drawer"
							autoFocus
							terminalId={getTerminalRuntimeId(scopeKey, activeTab.id)}
							title={activeTab.title}
							cwd={cwd}
							workspaceName={workspaceName}
							workspaceBranch={workspaceBranch}
							providerLabel={providerLabel}
							sessionState={sessionState}
							sessionId={sessionId}
							onSelectionChange={handleTerminalSelectionChange}
						/>
					) : (
						<div className="flex flex-1 items-center justify-center text-[12px] text-muted-foreground">
							{cwd ? t("terminalDock.empty") : t("terminalDock.noPath")}
						</div>
					)}
				</div>
			</div>

			<Dialog
				open={renamingTab !== null}
				onOpenChange={(nextOpen) => {
					if (!nextOpen) setRenamingTab(null);
				}}
			>
				<DialogContent className="sm:max-w-sm">
					<DialogHeader>
						<DialogTitle>{t("terminalDock.renameTitle")}</DialogTitle>
						<DialogDescription>{t("terminalDock.renameDescription")}</DialogDescription>
					</DialogHeader>
					<form className="grid gap-4" onSubmit={submitRenameTab}>
						<Input
							autoFocus
							value={tabTitleDraft}
							onChange={(event) => setTabTitleDraft(event.target.value)}
							maxLength={80}
							aria-label={t("terminalDock.renameInput")}
						/>
						<DialogFooter>
							<Button type="button" variant="outline" onClick={() => setRenamingTab(null)}>
								{t("terminalDock.cancelRename")}
							</Button>
							<Button type="submit" disabled={!tabTitleDraft.trim()}>
								{t("terminalDock.saveRename")}
							</Button>
						</DialogFooter>
					</form>
				</DialogContent>
			</Dialog>

			<Dialog
				open={closingTab !== null}
				onOpenChange={(nextOpen) => {
					if (!nextOpen) setClosingTab(null);
				}}
			>
				<DialogContent className="sm:max-w-md">
					<DialogHeader>
						<DialogTitle>{t("terminalDock.closeRunningTitle")}</DialogTitle>
						<DialogDescription>
							{t("terminalDock.closeRunningDescription", {
								process:
									closingSnapshot?.activityLabel ??
									closingTab?.title ??
									t("terminalDock.process"),
							})}
						</DialogDescription>
					</DialogHeader>
					<DialogFooter className="sm:justify-between">
						<Button
							type="button"
							variant="ghost"
							onClick={() => {
								setClosingTab(null);
								onOpenChange(false);
							}}
						>
							{t("terminalDock.hideInstead")}
						</Button>
						<div className="flex justify-end gap-2">
							<Button
								type="button"
								variant="outline"
								onClick={() => setClosingTab(null)}
							>
								{t("terminalDock.cancelClose")}
							</Button>
							<Button
								type="button"
								variant="destructive"
								onClick={() => {
									if (closingTab) removeTerminal(scopeKey, closingTab.id);
									setClosingTab(null);
								}}
							>
								{t("terminalDock.terminateAndClose")}
							</Button>
						</div>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</div>
	);
}
