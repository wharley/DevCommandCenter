import {
	ChevronDown,
	GripHorizontal,
	Maximize2,
	Minimize2,
	PanelRightClose,
	Pencil,
	Plus,
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
import { TerminalPanel } from "./terminal-panel";
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
	subscribeTerminalStore,
	type TerminalStatus,
} from "./terminal-store";
import type { TerminalScopeKind, TerminalScopeTarget } from "./terminal-scope";
import { cn } from "@/lib/utils";
import { getTerminalTabNavigationTarget } from "./terminal-tab-navigation";

const HEIGHT_STORAGE_KEY = "dcc-workbench-terminal-dock-height-v1";
const DEFAULT_HEIGHT_PX = 340;
const MIN_HEIGHT_PX = 220;
const COMPACT_MIN_HEIGHT_PX = 160;
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

function statusDotClass(status: TerminalStatus | "ready") {
	if (status === "running") return "bg-emerald-500";
	if (status === "starting") return "bg-amber-400";
	if (status === "error") return "bg-destructive";
	if (status === "exited") return "bg-muted-foreground";
	return "bg-muted-foreground/45";
}

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
	workspaceBranch: string;
	providerLabel: string | null;
	sessionState: string;
	sessionId: string | null;
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
	workspaceBranch,
	providerLabel,
	sessionState,
	sessionId,
	className,
}: WorkspaceTerminalDrawerProps) {
	const { t } = useTranslation("common");
	const dockRef = useRef<HTMLDivElement | null>(null);
	const dragRef = useRef<{ startY: number; startH: number } | null>(null);
	const releaseFitRef = useRef<(() => void) | null>(null);
	const terminalTabRefs = useRef(new Map<string, HTMLButtonElement>());
	const heightRef = useRef(DEFAULT_HEIGHT_PX);
	const [heightPx, setHeightPx] = useState(DEFAULT_HEIGHT_PX);
	const [terminalStatusVersion, setTerminalStatusVersion] = useState(0);
	const [renamingTab, setRenamingTab] = useState<{ id: string; title: string } | null>(null);
	const [tabTitleDraft, setTabTitleDraft] = useState("");

	const { tabs, activeId } = useProjectTerminals(scopeKey);
	const activeTab = tabs.find((tab) => tab.id === activeId) ?? tabs[0] ?? null;
	const atCap = tabs.length >= MAX_TERMINAL_TABS;
	const terminalStatuses = useMemo(
		() =>
			new Map<string, TerminalStatus | "ready">(
				tabs.map((tab): [string, TerminalStatus | "ready"] => [
					tab.id,
					getTerminalSnapshot(getTerminalRuntimeId(scopeKey, tab.id))?.status ??
						"ready",
				]),
			),
		[scopeKey, tabs, terminalStatusVersion],
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
									{scopeLabel}
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
										<span>{scope.label}</span>
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
							const status = terminalStatuses.get(tab.id) ?? "ready";
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
										onClick={() => setActiveTerminal(scopeKey, tab.id)}
										onKeyDown={(event) => handleTerminalTabKeyDown(event, tab.id)}
									>
										<span className="flex items-center gap-1.5">
											<span className={cn("size-1.5 rounded-full", statusDotClass(status))} />
											<span className="truncate">{tab.title}</span>
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
											removeTerminal(scopeKey, tab.id);
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
								<PanelRightClose className="size-4" />
							</Button>
						</TooltipTrigger>
					<TooltipContent side="bottom">{t("terminalDock.hideHint")}</TooltipContent>
					</Tooltip>
				</div>

				<div
					id={activeTab ? `terminal-panel-${activeTab.id}` : undefined}
					role="tabpanel"
					aria-labelledby={activeTab ? `terminal-tab-${activeTab.id}` : undefined}
					className="dcc-workbench-terminal-dock__panel flex min-h-0 flex-1 flex-col px-2 pb-2 pt-1"
				>
					{activeTab ? (
						<TerminalPanel
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
		</div>
	);
}
