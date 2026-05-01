import { Plus } from "lucide-react";
import {
	useEffect,
	useMemo,
	useRef,
	useState,
	type MouseEvent as ReactMouseEvent,
} from "react";
import { BranchToolbar } from "@/components/BranchToolbar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { SessionEventFeed } from "@/features/sessions/session-event-feed";
import type { RuntimeSessionSnapshot } from "@/features/sessions/session-workbench";
import { ProviderCatalogCard } from "@/features/providers/provider-catalog-card";
import { WorkspaceCommitButton } from "@/features/commit";
import { resolveCommitMode } from "@/features/commit/WorkspaceCommitButton.logic";
import { EmptyState } from "@/features/panel";
import { cn } from "@/lib/utils";
import type { CoreEvent, ProviderCatalog } from "@dcc/contracts";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";

type WorkspaceInspectorSidebarProps = {
	providerCatalog: ProviderCatalog | null;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	workspaceId: string | null;
	workspaceName: string | null;
	workspaceBranch: string | null;
	workspacePath: string | null;
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	sessionState: string;
	sessionId: string | null;
	sessionEvents: CoreEvent[];
};

const MIN_SECTION_HEIGHT = 128;
const MAX_SECTION_HEIGHT = 320;
const INITIAL_CHANGES_HEIGHT = 168;
const INITIAL_ACTIONS_HEIGHT = 208;

type ResizeTarget = "changes" | "actions";
type TabsKey = "setup" | "run" | "terminal";

function clamp(value: number, min: number, max: number) {
	return Math.min(max, Math.max(min, value));
}

function gitSectionHeaderHighlightClass(mode: ReturnType<typeof resolveCommitMode>) {
	switch (mode) {
		case "fix":
		case "closed":
			return "bg-[var(--workspace-pr-closed-header-bg)]";
		case "resolve-conflicts":
			return "bg-[var(--workspace-pr-conflicts-header-bg)]";
		case "merge":
		case "open-pr":
			return "bg-[var(--workspace-pr-open-header-bg)]";
		case "merged":
			return "bg-[var(--workspace-pr-merged-header-bg)]";
		default:
			return "bg-muted/25";
	}
}

function ResizeHandle({
	label,
	onMouseDown,
}: {
	label: string;
	onMouseDown: (event: ReactMouseEvent<HTMLButtonElement>) => void;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			onMouseDown={onMouseDown}
			className="group flex h-2 shrink-0 cursor-row-resize items-center justify-center bg-sidebar outline-none"
		>
			<span className="h-px w-full bg-border/70 transition-colors group-hover:bg-foreground/30 group-focus-visible:bg-foreground/40" />
		</button>
	);
}

/**
 * Right rail: inspector-first chrome with git context, session actions, and tabs.
 * Terminal remains in the main workbench bottom drawer.
 */
export function WorkspaceInspectorSidebar({
	providerCatalog,
	sessionSnapshot,
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	selectedProviderLabel,
	selectedModelLabel,
	sessionState,
	sessionId,
	sessionEvents,
}: WorkspaceInspectorSidebarProps) {
	const hasWorkspace = Boolean(workspaceId && workspaceName && workspaceBranch);
	const pathLine =
		workspacePath && workspacePath.length > 0
			? workspacePath.length > 56
				? `…${workspacePath.slice(-55)}`
				: workspacePath
			: null;
	const commitMode = resolveCommitMode(workspaceBranch ?? "");
	const headerToneClass = gitSectionHeaderHighlightClass(commitMode);
	const [changesHeight, setChangesHeight] = useState(INITIAL_CHANGES_HEIGHT);
	const [actionsHeight, setActionsHeight] = useState(INITIAL_ACTIONS_HEIGHT);
	const [activeTab, setActiveTab] = useState<TabsKey>("setup");
	const [isTabsHovered, setIsTabsHovered] = useState(false);
	const [isTabsZoomed, setIsTabsZoomed] = useState(false);
	const dragRef = useRef<{
		target: ResizeTarget;
		startY: number;
		startHeight: number;
	} | null>(null);

	useEffect(() => {
		if (!isTabsHovered) {
			setIsTabsZoomed(false);
			return;
		}

		const timeout = window.setTimeout(() => setIsTabsZoomed(true), 300);
		return () => window.clearTimeout(timeout);
	}, [isTabsHovered]);

	useEffect(() => {
		const onPointerMove = (event: MouseEvent) => {
			const drag = dragRef.current;
			if (!drag) {
				return;
			}

			const delta = event.clientY - drag.startY;
			if (drag.target === "changes") {
				setChangesHeight(clamp(drag.startHeight + delta, MIN_SECTION_HEIGHT, MAX_SECTION_HEIGHT));
			} else {
				setActionsHeight(clamp(drag.startHeight + delta, MIN_SECTION_HEIGHT, MAX_SECTION_HEIGHT));
			}
		};

		const onPointerUp = () => {
			dragRef.current = null;
			window.removeEventListener("mousemove", onPointerMove);
			window.removeEventListener("mouseup", onPointerUp);
		};

		if (dragRef.current) {
			window.addEventListener("mousemove", onPointerMove);
			window.addEventListener("mouseup", onPointerUp);
		}

		return () => {
			window.removeEventListener("mousemove", onPointerMove);
			window.removeEventListener("mouseup", onPointerUp);
		};
	}, [changesHeight, actionsHeight]);

	const workspaceSummary = useMemo(
		() => [
			{ label: "Workspace id", value: workspaceId ?? "" },
			{ label: "Provider", value: selectedProviderLabel ?? "Unknown provider" },
			{ label: "Runtime", value: sessionState },
		],
		[sessionState, selectedProviderLabel, workspaceId],
	);

	if (!hasWorkspace) {
		return (
			<div className="dcc-inspector flex h-full min-h-0 flex-col overflow-hidden text-foreground">
				<div className="flex min-h-0 flex-1 items-center justify-center px-4 py-6">
					<EmptyState
						title="No workspace selected"
						description="Open or create a workspace to inspect git state, provider context, and session activity."
					/>
				</div>
			</div>
		);
	}

	return (
		<div
			className="dcc-inspector flex h-full min-h-0 flex-col overflow-hidden text-foreground"
			data-dcc-inspector-root
			data-tabs-zoomed={isTabsZoomed ? "true" : undefined}
		>
			<section
				className={cn(
					"flex shrink-0 flex-col overflow-hidden border-b border-border/60",
					headerToneClass,
				)}
				style={{ height: `${changesHeight}px` }}
			>
				<div className={cn("relative flex items-center justify-between gap-3 px-3 py-2", "border-b border-border/40")}>
					<div className="min-w-0">
						<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							Git
						</p>
						<p
							className="truncate text-[13px] font-medium leading-tight"
							title={workspaceName ?? undefined}
						>
							{workspaceName ?? "Workspace"}
						</p>
					</div>
					<div className="flex shrink-0 items-center gap-2">
						<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
							{commitMode}
						</Badge>
						<WorkspaceCommitButton mode={commitMode} />
					</div>
					<div className="pointer-events-none absolute inset-x-0 bottom-0 h-px bg-[linear-gradient(90deg,transparent,color-mix(in_oklch,var(--foreground)_28%,transparent),transparent)] bg-[length:200%_100%] motion-safe:animate-shine" />
				</div>

				<div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden p-3">
						<BranchToolbar branch={workspaceBranch ?? ""} workspacePath={workspacePath} />
					<div className="grid gap-2 sm:grid-cols-3">
						{workspaceSummary.map((item) => (
							<Card key={item.label} className="border-border/60 shadow-none">
								<CardContent className="p-3">
									<p className="text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
										{item.label}
									</p>
									<p className="mt-1 truncate text-[13px] text-foreground">{item.value}</p>
								</CardContent>
							</Card>
						))}
					</div>
					{pathLine ? (
						<p className="truncate text-[11px] text-muted-foreground" title={workspacePath ?? undefined}>
							{pathLine}
						</p>
					) : null}
				</div>
			</section>

			<ResizeHandle
				label="Resize changes section"
				onMouseDown={(event) => {
					event.preventDefault();
					dragRef.current = {
						target: "changes",
						startY: event.clientY,
						startHeight: changesHeight,
					};
				}}
			/>

			<section className="flex min-h-0 flex-1 flex-col overflow-hidden border-b border-border/60">
				<div className="flex items-center justify-between gap-3 border-b border-border/40 px-3 py-2">
					<div>
						<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							Actions
						</p>
						<p className="text-[13px] font-medium leading-tight text-foreground">
							Provider and session state
						</p>
					</div>
					<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
						{providerCatalog?.providers.length ?? 0} providers
					</Badge>
				</div>

				<div className="dcc-inspector-sidebar__body flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-3">
					<ProviderCatalogCard catalog={providerCatalog} />
					<Card className="dcc-session-state-card border-border">
						<div className="flex items-center justify-between border-b border-border/40 px-4 py-3">
							<h3 className="text-sm font-medium">Session state</h3>
							<Badge variant="outline">{sessionSnapshot?.lastTurnState ?? "pending"}</Badge>
						</div>
						<CardContent className="dcc-runtime-feed__content pt-0">
							{sessionSnapshot ? (
								<div className="dcc-runtime-feed__list">
									<div className="dcc-runtime-feed__row">
										<strong>Projection</strong>
										<small>
											turns {sessionSnapshot.turnCount} · checkpoints{" "}
											{sessionSnapshot.checkpointCount}
										</small>
									</div>
									<div className="dcc-runtime-feed__row">
										<strong>Provider</strong>
										<small>{sessionSnapshot.providerId}</small>
									</div>
									<div className="dcc-runtime-feed__row">
										<strong>Last turn</strong>
										<small>{sessionSnapshot.lastTurnPrompt ?? "No turn yet"}</small>
									</div>
								</div>
							) : (
								<p className="dcc-card__description text-muted-foreground">
									No active session. Start one from the composer.
								</p>
							)}
						</CardContent>
					</Card>
				</div>
			</section>

			<ResizeHandle
				label="Resize actions section"
				onMouseDown={(event) => {
					event.preventDefault();
					dragRef.current = {
						target: "actions",
						startY: event.clientY,
						startHeight: actionsHeight,
					};
				}}
			/>

			<section className="flex min-h-0 flex-1 flex-col overflow-hidden">
				<div className="flex items-center justify-between gap-3 border-b border-border/40 px-3 py-2">
					<div>
						<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							Tabs
						</p>
						<p className="text-[13px] font-medium leading-tight text-foreground">
							Setup, run, and terminal context
						</p>
					</div>
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						aria-label="Add terminal tab"
						title="Add terminal tab"
						onClick={() => {
							// Placeholder for the future terminal add flow.
						}}
						className="text-muted-foreground hover:text-foreground"
					>
						<Plus className="size-4" strokeWidth={2} aria-hidden />
					</Button>
				</div>

				<Tabs
					value={activeTab}
					onValueChange={(value) => {
						if (value === "setup" || value === "run" || value === "terminal") {
							setActiveTab(value);
						}
					}}
					className="flex min-h-0 flex-1 flex-col gap-0"
				>
					<div className="flex items-center justify-between border-b border-border/40 bg-muted/20 px-2">
						<TabsList variant="line" className="h-8 flex-1 justify-start gap-0 border-0 bg-transparent p-0">
							<TabsTrigger value="setup" className="h-8 rounded-none px-3 text-[12px]">
								Setup
							</TabsTrigger>
							<TabsTrigger value="run" className="h-8 rounded-none px-3 text-[12px]">
								Run
							</TabsTrigger>
							<TabsTrigger value="terminal" className="h-8 rounded-none px-3 text-[12px]">
								Terminal 1
							</TabsTrigger>
						</TabsList>
						<div className="px-2 text-[11px] text-muted-foreground">Hover to zoom</div>
					</div>

					<div
						className={cn(
							"relative min-h-0 flex-1 overflow-hidden bg-sidebar transition-[transform,box-shadow] duration-400 ease-[cubic-bezier(0.32,0.72,0,1)]",
							isTabsZoomed && "z-50 shadow-[0_30px_60px_-20px_rgba(0,0,0,0.35)]",
						)}
						style={{
							transformOrigin: "top right",
							transform: isTabsZoomed ? "scale(2)" : "scale(1)",
						}}
						onMouseEnter={() => setIsTabsHovered(true)}
						onMouseLeave={() => setIsTabsHovered(false)}
					>
						<div className="flex min-h-0 flex-1 flex-col overflow-hidden p-3">
							{activeTab === "setup" ? (
								<div className="grid gap-3">
									<Card className="border-border/60">
										<CardContent className="space-y-2 p-4">
											<p className="text-[12px] font-medium text-foreground">Workspace setup</p>
											<p className="text-[12px] leading-relaxed text-muted-foreground">
												The current workspace is ready for the setup and run tabs to surface the active thread state.
											</p>
										</CardContent>
									</Card>
									<Card className="border-border/60">
										<CardContent className="space-y-2 p-4">
											<p className="text-[12px] font-medium text-foreground">Branch context</p>
											<p className="text-[12px] leading-relaxed text-muted-foreground">
												{workspaceBranch || "No branch available"} · {workspacePath ?? "Workspace path unavailable"}
											</p>
										</CardContent>
									</Card>
								</div>
							) : null}

							{activeTab === "run" ? (
								<div className="min-h-0 flex-1">
									<SessionEventFeed events={sessionEvents} compact />
								</div>
							) : null}

							{activeTab === "terminal" ? (
								<div className="grid min-h-0 flex-1 gap-3">
									<Card className="border-border/60">
										<CardContent className="space-y-2 p-4">
											<p className="text-[12px] font-medium text-foreground">Terminal tab</p>
											<p className="text-[12px] leading-relaxed text-muted-foreground">
												Terminal instances live in the workbench drawer for now. This tab keeps the inspector contract in place for the future split.
											</p>
										</CardContent>
									</Card>
									<Card className="border-border/60">
										<CardContent className="space-y-2 p-4">
											<p className="text-[12px] font-medium text-foreground">Live state</p>
											<p className="text-[12px] leading-relaxed text-muted-foreground">
												Session {sessionId ?? "not started"} · {sessionState}
											</p>
										</CardContent>
									</Card>
								</div>
							) : null}
						</div>
					</div>
				</Tabs>
			</section>
		</div>
	);
}
