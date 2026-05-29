import { GripVertical, Maximize2, Minimize2, PanelRightClose, Plus, X } from "lucide-react";
import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { Button } from "@/components/ui/button";
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
	MAX_TERMINAL_TABS,
	removeTerminal,
	setActiveTerminal,
	useProjectTerminals,
} from "./terminal-tabs-store";
import { cn } from "@/lib/utils";

const WIDTH_STORAGE_KEY = "dcc-workbench-terminal-dock-width-v1";
const DEFAULT_WIDTH_PX = 520;
const MIN_WIDTH_PX = 360;
/** Keep the chat column breathable — never let the dock crush it below this. */
const MIN_CHAT_WIDTH_PX = 360;

function maxDockWidth(): number {
	return Math.max(MIN_WIDTH_PX, window.innerWidth - MIN_CHAT_WIDTH_PX);
}

function clampWidth(px: number, maxPx: number) {
	const safe = Number.isFinite(px) ? px : DEFAULT_WIDTH_PX;
	return Math.round(
		Math.min(Math.max(safe, MIN_WIDTH_PX), Math.max(maxPx, MIN_WIDTH_PX)),
	);
}

export type WorkspaceTerminalDrawerProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	/** Expanded = terminal takes over the full workbench (chat hidden). */
	expanded: boolean;
	onExpandedChange: (expanded: boolean) => void;
	/** Terminals are scoped per project (one set of tabs per project root). */
	projectKey: string;
	/** Project root (`rootPath`) — terminals open here, NOT inside the worktree. */
	rootPath: string | null;
	workspaceName: string;
	workspaceBranch: string;
	providerLabel: string | null;
	sessionState: string;
	sessionId: string | null;
	className?: string;
};

/**
 * Side dock — general-purpose terminals (tabs) rooted at the project, outside the worktree.
 *
 * Default: a resizable right-hand split so chat + terminal stay visible together.
 * Expanded: promotes to a full-bleed surface (chat hidden) for terminal-only focus.
 */
export function WorkspaceTerminalDrawer({
	open,
	onOpenChange,
	expanded,
	onExpandedChange,
	projectKey,
	rootPath,
	workspaceName,
	workspaceBranch,
	providerLabel,
	sessionState,
	sessionId,
	className,
}: WorkspaceTerminalDrawerProps) {
	const dragRef = useRef<{ startX: number; startW: number } | null>(null);
	const releaseFitRef = useRef<(() => void) | null>(null);
	const widthRef = useRef(DEFAULT_WIDTH_PX);
	const [widthPx, setWidthPx] = useState(DEFAULT_WIDTH_PX);

	const { tabs, activeId } = useProjectTerminals(projectKey);
	const activeTab = tabs.find((tab) => tab.id === activeId) ?? tabs[0] ?? null;
	const atCap = tabs.length >= MAX_TERMINAL_TABS;

	// Make sure an open dock always has at least one terminal to show.
	useEffect(() => {
		if (open) {
			ensureTerminal(projectKey);
		}
	}, [open, projectKey]);

	useEffect(() => {
		widthRef.current = widthPx;
	}, [widthPx]);

	useLayoutEffect(() => {
		try {
			const raw = localStorage.getItem(WIDTH_STORAGE_KEY);
			const n = raw ? Number.parseInt(raw, 10) : DEFAULT_WIDTH_PX;
			if (Number.isFinite(n)) {
				setWidthPx(clampWidth(n, maxDockWidth()));
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
		(clientX: number) => {
			dragRef.current = { startX: clientX, startW: widthPx };
			// Hold xterm refits until the drag ends, then fit once — avoids
			// flooding the PTY with resize events on every mousemove frame.
			releaseFitRef.current = suspendTerminalFit();
		},
		[widthPx],
	);

	const applyResizeMove = useCallback((clientX: number) => {
		const drag = dragRef.current;
		if (!drag) {
			return;
		}
		// Dock sits on the right: dragging the handle left grows it.
		const delta = drag.startX - clientX;
		const next = clampWidth(drag.startW + delta, maxDockWidth());
		setWidthPx(next);
	}, []);

	const endResize = useCallback(() => {
		dragRef.current = null;
		releaseFitRef.current?.();
		releaseFitRef.current = null;
		try {
			localStorage.setItem(WIDTH_STORAGE_KEY, String(widthRef.current));
		} catch {
			/* ignore */
		}
	}, []);

	useEffect(() => {
		const onMove = (event: MouseEvent) => {
			if (dragRef.current) {
				applyResizeMove(event.clientX);
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

	// Keep width within bounds when the window shrinks.
	useEffect(() => {
		const onResize = () => {
			setWidthPx((current) => clampWidth(current, maxDockWidth()));
		};
		window.addEventListener("resize", onResize);
		return () => window.removeEventListener("resize", onResize);
	}, []);

	if (!open) {
		return null;
	}

	return (
		<div
			className={cn(
				"dcc-workbench-terminal-dock flex min-h-0 min-w-0 flex-row overflow-hidden bg-card text-card-foreground",
				expanded ? "min-w-0 flex-1" : "shrink-0 border-l border-border",
				className,
			)}
			style={expanded ? undefined : { width: widthPx }}
			role="region"
			aria-label="Terminal dock"
		>
			{!expanded ? (
				<Tooltip>
					<TooltipTrigger asChild>
						<button
							type="button"
							className={cn(
								"dcc-workbench-terminal-dock__resize group flex w-1.5 shrink-0 cursor-ew-resize items-center justify-center border-r border-border/60 bg-muted/20",
								"hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
							)}
							aria-label="Resize terminal dock"
							onMouseDown={(event) => {
								event.preventDefault();
								beginResize(event.clientX);
							}}
						>
							<GripVertical className="size-3 text-muted-foreground/0 transition-colors group-hover:text-muted-foreground" />
						</button>
					</TooltipTrigger>
					<TooltipContent side="left">Drag to resize</TooltipContent>
				</Tooltip>
			) : null}

			<div className="flex min-h-0 min-w-0 flex-1 flex-col">
				<div className="flex h-10 shrink-0 items-center gap-2 border-b border-border/60 px-2">
					<div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
						{tabs.map((tab) => {
							const isActive = tab.id === activeTab?.id;
							return (
								<div
									key={tab.id}
									className={cn(
										"group/tab flex shrink-0 items-center gap-1 rounded-md border px-2 py-1 text-[12px] transition-colors",
										isActive
											? "border-border bg-background text-foreground"
											: "border-transparent text-muted-foreground hover:bg-muted/50 hover:text-foreground",
									)}
								>
									<button
										type="button"
										className="max-w-[9rem] truncate"
										onClick={() => setActiveTerminal(projectKey, tab.id)}
									>
										{tab.title}
									</button>
									<button
										type="button"
										aria-label={`Close ${tab.title}`}
										className="rounded p-0.5 text-muted-foreground/60 opacity-0 transition-opacity hover:bg-muted hover:text-foreground group-hover/tab:opacity-100"
										onClick={(event) => {
											event.stopPropagation();
											removeTerminal(projectKey, tab.id);
										}}
									>
										<X className="size-3" />
									</button>
								</div>
							);
						})}
						<Tooltip>
							<TooltipTrigger asChild>
								<Button
									type="button"
									size="sm"
									variant="ghost"
									className="h-7 shrink-0 px-1.5"
									aria-label="New terminal tab"
									disabled={atCap}
									onClick={() => addTerminal(projectKey)}
								>
									<Plus className="size-4" />
								</Button>
							</TooltipTrigger>
							<TooltipContent side="bottom">
								{atCap ? `Max ${MAX_TERMINAL_TABS} terminals` : "New terminal"}
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
								aria-label={expanded ? "Collapse terminal to split" : "Expand terminal"}
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
							{expanded ? "Collapse to split (Esc)" : "Expand terminal"}
						</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								size="sm"
								variant="outline"
								className="h-8 shrink-0 px-2"
								aria-label="Hide terminal dock"
								onClick={() => onOpenChange(false)}
							>
								<PanelRightClose className="size-4" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">Hide terminal (Esc)</TooltipContent>
					</Tooltip>
				</div>

				<div className="dcc-workbench-terminal-dock__panel flex min-h-0 flex-1 flex-col px-2 pb-2 pt-1">
					{activeTab ? (
						<TerminalPanel
							key={activeTab.id}
							variant="drawer"
							terminalId={activeTab.id}
							title={activeTab.title}
							cwd={rootPath}
							workspaceName={workspaceName}
							workspaceBranch={workspaceBranch}
							providerLabel={providerLabel}
							sessionState={sessionState}
							sessionId={sessionId}
						/>
					) : (
						<div className="flex flex-1 items-center justify-center text-[12px] text-muted-foreground">
							No terminal open
						</div>
					)}
				</div>
			</div>
		</div>
	);
}
