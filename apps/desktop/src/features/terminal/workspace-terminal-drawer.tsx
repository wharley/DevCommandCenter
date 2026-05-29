import { ChevronDown, GripHorizontal, Plus, X } from "lucide-react";
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

const HEIGHT_STORAGE_KEY = "dcc-workbench-terminal-drawer-height-v1";
const DEFAULT_HEIGHT_PX = 300;
const MIN_HEIGHT_PX = 180;

function clampHeight(px: number, maxPx: number) {
	const safe = Number.isFinite(px) ? px : DEFAULT_HEIGHT_PX;
	return Math.round(
		Math.min(Math.max(safe, MIN_HEIGHT_PX), Math.max(maxPx, MIN_HEIGHT_PX)),
	);
}

export type WorkspaceTerminalDrawerProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
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

/** Bottom workbench drawer — general-purpose terminals (tabs) rooted at the project, outside the worktree. */
export function WorkspaceTerminalDrawer({
	open,
	onOpenChange,
	projectKey,
	rootPath,
	workspaceName,
	workspaceBranch,
	providerLabel,
	sessionState,
	sessionId,
	className,
}: WorkspaceTerminalDrawerProps) {
	const drawerRef = useRef<HTMLDivElement | null>(null);
	const dragRef = useRef<{ startY: number; startH: number } | null>(null);
	const heightRef = useRef(DEFAULT_HEIGHT_PX);
	const [heightPx, setHeightPx] = useState(DEFAULT_HEIGHT_PX);

	const { tabs, activeId } = useProjectTerminals(projectKey);
	const activeTab = tabs.find((tab) => tab.id === activeId) ?? tabs[0] ?? null;
	const atCap = tabs.length >= MAX_TERMINAL_TABS;

	// Make sure an open drawer always has at least one terminal to show.
	useEffect(() => {
		if (open) {
			ensureTerminal(projectKey);
		}
	}, [open, projectKey]);

	useEffect(() => {
		heightRef.current = heightPx;
	}, [heightPx]);

	useLayoutEffect(() => {
		try {
			const raw = localStorage.getItem(HEIGHT_STORAGE_KEY);
			const n = raw ? Number.parseInt(raw, 10) : DEFAULT_HEIGHT_PX;
			if (Number.isFinite(n)) {
				setHeightPx(n);
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
			if (event.key === "Escape") {
				event.preventDefault();
				onOpenChange(false);
			}
		};

		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, [open, onOpenChange]);

	const beginResize = useCallback(
		(clientY: number) => {
			dragRef.current = { startY: clientY, startH: heightPx };
		},
		[heightPx],
	);

	const applyResizeMove = useCallback((clientY: number) => {
		const drag = dragRef.current;
		if (!drag) {
			return;
		}

		const cappedMax = Math.max(MIN_HEIGHT_PX, Math.floor(window.innerHeight * 0.72));
		const delta = drag.startY - clientY;
		const next = clampHeight(drag.startH + delta, cappedMax);
		setHeightPx(next);
	}, []);

	const endResize = useCallback(() => {
		dragRef.current = null;
		try {
			localStorage.setItem(HEIGHT_STORAGE_KEY, String(heightRef.current));
		} catch {
			/* ignore */
		}
	}, []);

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

	if (!open) {
		return null;
	}

	return (
		<div
			ref={drawerRef}
			className={cn(
				"thread-terminal-drawer dcc-workbench-terminal-drawer flex shrink-0 flex-col overflow-hidden rounded-t-xl border-x border-t border-border bg-card text-card-foreground shadow-lg",
				className,
			)}
			style={{ height: heightPx }}
			role="region"
			aria-label="Terminal drawer"
		>
			<Tooltip>
				<TooltipTrigger asChild>
					<button
						type="button"
						className={cn(
							"dcc-workbench-terminal-drawer__resize group flex h-5 w-full shrink-0 cursor-ns-resize items-center justify-center border-b border-border/80 bg-muted/30",
							"hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
						)}
						aria-label="Resize terminal drawer"
						onMouseDown={(event) => {
							event.preventDefault();
							beginResize(event.clientY);
						}}
					>
						<GripHorizontal className="size-3.5 text-muted-foreground group-hover:text-foreground" />
					</button>
				</TooltipTrigger>
				<TooltipContent side="top">Drag to resize</TooltipContent>
			</Tooltip>

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
						<TooltipContent side="top">
							{atCap ? `Max ${MAX_TERMINAL_TABS} terminals` : "New terminal"}
						</TooltipContent>
					</Tooltip>
				</div>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							size="sm"
							variant="outline"
							className="h-8 shrink-0 px-2"
							aria-label="Hide terminal drawer"
							onClick={() => onOpenChange(false)}
						>
							<ChevronDown className="size-4" />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="top">Hide terminal (Esc)</TooltipContent>
				</Tooltip>
			</div>

			<div className="dcc-workbench-terminal-drawer__panel flex min-h-0 flex-1 flex-col px-2 pb-2 pt-1">
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
	);
}
