import {
	useCallback,
	useEffect,
	useMemo,
	useState,
	type KeyboardEventHandler,
	type MouseEventHandler,
} from "react";
import { Toaster } from "sonner";
import { cn } from "@/lib/utils";
import {
	MAX_INSPECTOR_WIDTH,
	MAX_SIDEBAR_WIDTH,
	MIN_INSPECTOR_WIDTH,
	MIN_SIDEBAR_WIDTH,
	SIDEBAR_RESIZE_HIT_AREA,
} from "./shell/layout";
import { useShellPanels } from "./shell/hooks/useShellPanels";
import { useZoom } from "./shell/use-zoom";
import {
	WorkspacesSidebar,
	WorkspaceCommandPalette,
	CreateWorkspaceDialog,
	useWorkspacesPanel,
} from "./features/workspaces";
import { WorkspaceInspectorSidebar } from "./features/inspector";
import {
	SessionWorkbench,
	type RuntimeSessionSnapshot,
} from "./features/sessions/session-workbench";
import { useSessionEventFeed } from "./features/sessions/use-session-event-feed";
import { listProviders } from "./lib/provider-api";
import {
	abortRun,
	resumeSession,
	sendTurn,
	startThread,
} from "./lib/session-api";
import type {
	ProviderCatalog,
} from "@dcc/contracts";
import { useAppearance } from "./components/theme-provider";

function ResizeSeparator({
	side,
	widthAt,
	ariaLabel,
	ariaMin,
	ariaMax,
	ariaNow,
	isActive,
	onMouseDown,
	onKeyDown,
}: {
	side: "left" | "right";
	widthAt: number;
	ariaLabel: string;
	ariaMin: number;
	ariaMax: number;
	ariaNow: number;
	isActive: boolean;
	onMouseDown: MouseEventHandler<HTMLDivElement>;
	onKeyDown: KeyboardEventHandler<HTMLDivElement>;
}) {
	return (
		<div
			role="separator"
			tabIndex={0}
			aria-label={ariaLabel}
			aria-orientation="vertical"
			aria-valuemin={ariaMin}
			aria-valuemax={ariaMax}
			aria-valuenow={ariaNow}
			onMouseDown={onMouseDown}
			onKeyDown={onKeyDown}
			className="group absolute inset-y-0 z-30 cursor-ew-resize touch-none outline-none transition-[width,background-color,box-shadow]"
			style={{
				[side === "left" ? "left" : "right"]:
					side === "left"
						? `${Math.max(0, widthAt - SIDEBAR_RESIZE_HIT_AREA / 2)}px`
						: `${Math.max(0, widthAt - SIDEBAR_RESIZE_HIT_AREA)}px`,
				width: `${SIDEBAR_RESIZE_HIT_AREA}px`,
			}}
		>
			<span
				aria-hidden="true"
				className={cn(
					"pointer-events-none absolute inset-y-0 left-1/2 -translate-x-1/2",
					isActive
						? "w-[2px] bg-foreground/80 shadow-[0_0_12px_rgba(0,0,0,0.12)] dark:shadow-[0_0_12px_rgba(255,255,255,0.16)]"
						: "w-px bg-border group-hover:w-[2px] group-hover:bg-muted-foreground/75 group-focus-visible:w-[2px] group-focus-visible:bg-muted-foreground/75",
				)}
			/>
		</div>
	);
}

export default function App() {
	useZoom(1);

	const {
		handleResizeKeyDown,
		handleResizeStart,
		inspectorWidth,
		inspectorCollapsed,
		isInspectorResizing,
		isSidebarResizing,
		sidebarCollapsed,
		sidebarWidth,
		setSidebarCollapsed,
	} = useShellPanels();
	const {
		allWorkspaces,
		createWorkspace,
		filteredWorkspaces,
		isCreatingWorkspace,
		selectedWorkspace,
		selectedWorkspaceId,
		setSelectedWorkspaceId,
	} = useWorkspacesPanel();
	const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);
	const [isCreateWorkspaceOpen, setIsCreateWorkspaceOpen] = useState(false);
	const { events: sessionEvents } = useSessionEventFeed();
	const [providerCatalog, setProviderCatalog] = useState<ProviderCatalog | null>(
		null,
	);
	const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
		null,
	);
	const [sessionSnapshot, setSessionSnapshot] =
		useState<RuntimeSessionSnapshot | null>(null);
	const { theme } = useAppearance();
	const providerChoices = providerCatalog?.providers ?? [];
	const selectedWorkspacePath =
		selectedWorkspace.worktreePath ?? selectedWorkspace.rootPath ?? null;
	const selectedProvider = useMemo(
		() =>
			providerChoices.find((provider) => provider.id === selectedProviderId) ??
			providerChoices[0] ??
			null,
		[providerChoices, selectedProviderId],
	);

	useEffect(() => {
		let disposed = false;
		void listProviders().then((result) => {
			if (!disposed) {
				setProviderCatalog(result.catalog);
			}
		});

		return () => {
			disposed = true;
		};
	}, []);

	useEffect(() => {
		if (providerChoices.length === 0) {
			return;
		}

		setSelectedProviderId((current) => {
			if (current && providerChoices.some((provider) => provider.id === current)) {
				return current;
			}

			return providerChoices.find((provider) => provider.stable)?.id ?? providerChoices[0].id;
		});
	}, [providerChoices]);

	useEffect(() => {
		setSessionSnapshot(null);
	}, [selectedWorkspace.id]);

	const handleStartSession = useCallback(async () => {
		if (!selectedProvider) {
			return;
		}

		const result = await startThread({
			workspaceId: selectedWorkspace.id,
			projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
			providerId: selectedProvider.id,
			title: `${selectedWorkspace.name} session`,
		});

		setSessionSnapshot({
			sessionId: result.session.id,
			projectId: result.session.projectId,
			workspaceId: result.session.workspaceId,
			providerId: result.session.providerId,
			state: result.projection.state,
			turnCount: result.projection.turnCount,
			checkpointCount: result.projection.checkpointCount,
		});
	}, [selectedProvider, selectedWorkspace.id, selectedWorkspace.name, selectedWorkspace.projectId]);

	const handleSubmitPrompt = useCallback(async (prompt: string) => {
		const trimmedPrompt = prompt.trim();
		if (trimmedPrompt.length === 0) {
			return;
		}

		let currentSession = sessionSnapshot;
		if (!currentSession) {
			if (!selectedProvider) {
				return;
			}

			const started = await startThread({
				workspaceId: selectedWorkspace.id,
				projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
				providerId: selectedProvider.id,
				title: `${selectedWorkspace.name} session`,
			});

			currentSession = {
				sessionId: started.session.id,
				projectId: started.session.projectId,
				workspaceId: started.session.workspaceId,
				providerId: started.session.providerId,
				state: started.projection.state,
				turnCount: started.projection.turnCount,
				checkpointCount: started.projection.checkpointCount,
			};
			setSessionSnapshot(currentSession);
		}

		const result = await sendTurn({
			sessionId: currentSession.sessionId,
			prompt: trimmedPrompt,
		});

		setSessionSnapshot({
			sessionId: result.session.id,
			projectId: result.session.projectId,
			workspaceId: result.session.workspaceId,
			providerId: result.session.providerId,
			state: result.projection.state,
			turnCount: result.projection.turnCount,
			checkpointCount: result.projection.checkpointCount,
			lastTurnPrompt: result.turn.content,
			lastTurnState: result.turn.state,
		});
	}, [
		selectedProvider,
		selectedWorkspace.id,
		selectedWorkspace.name,
		selectedWorkspace.projectId,
		sessionSnapshot,
	]);

	const handleResumeSession = useCallback(async () => {
		if (!sessionSnapshot) {
			return;
		}

		const result = await resumeSession({ sessionId: sessionSnapshot.sessionId });
		setSessionSnapshot((current: RuntimeSessionSnapshot | null) =>
			current
				? {
						...current,
						state: result.projection.state,
						turnCount: result.projection.turnCount,
						checkpointCount: result.projection.checkpointCount,
					}
				: current,
		);
	}, [sessionSnapshot]);

	const handleAbortSession = useCallback(async () => {
		if (!sessionSnapshot) {
			return;
		}

		const result = await abortRun({
			sessionId: sessionSnapshot.sessionId,
			reason: "Stopped from shell",
		});
		setSessionSnapshot((current: RuntimeSessionSnapshot | null) =>
			current
				? {
						...current,
						state: result.projection.state,
						turnCount: result.projection.turnCount,
						checkpointCount: result.projection.checkpointCount,
					}
				: current,
		);
	}, [sessionSnapshot]);

	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			const isShortcut =
				(event.metaKey || event.ctrlKey) &&
				event.key.toLowerCase() === "k" &&
				!event.shiftKey &&
				!event.altKey;

			if (!isShortcut) {
				return;
			}

			event.preventDefault();
			setIsCommandPaletteOpen(true);
		};

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, []);

	const sidebarRailWidth = sidebarCollapsed ? 76 : sidebarWidth;

	return (
		<>
			<main
				aria-label="Application shell"
				className="relative h-screen overflow-hidden bg-background font-sans text-foreground antialiased"
			>
				<div className="relative flex h-full min-h-0 bg-background">
					<aside
						aria-label="Workspace sidebar"
						data-dcc-sidebar-root
						className="relative flex h-full shrink-0 flex-col overflow-hidden bg-sidebar"
						style={{ width: `${sidebarRailWidth}px` }}
					>
						<WorkspacesSidebar
							collapsed={sidebarCollapsed}
							isCreatingWorkspace={isCreatingWorkspace}
							onSelectWorkspace={setSelectedWorkspaceId}
							onCreateWorkspace={() => setIsCreateWorkspaceOpen(true)}
							onToggleCollapsed={() => setSidebarCollapsed((value) => !value)}
							selectedWorkspaceId={selectedWorkspaceId}
							workspaces={filteredWorkspaces}
						/>
					</aside>

					<ResizeSeparator
						side="left"
						widthAt={sidebarRailWidth}
						ariaLabel="Resize sidebar"
						ariaMin={MIN_SIDEBAR_WIDTH}
						ariaMax={MAX_SIDEBAR_WIDTH}
						ariaNow={sidebarWidth}
						isActive={isSidebarResizing}
						onMouseDown={handleResizeStart("sidebar")}
						onKeyDown={handleResizeKeyDown("sidebar")}
					/>

					<section
						aria-label="Workspace panel"
						className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-background"
					>
						<div
							aria-label="Workspace panel drag region"
							data-tauri-drag-region
							className="absolute inset-x-0 top-0 z-10 h-9 bg-transparent"
						/>
						<div
							aria-label="Workspace viewport"
							className="flex min-h-0 flex-1 flex-col bg-background"
						>
							<WorkspaceCommandPalette
								open={isCommandPaletteOpen}
								onOpenChange={setIsCommandPaletteOpen}
								workspaces={allWorkspaces}
								selectedWorkspaceId={selectedWorkspace.id}
								onSelectWorkspace={setSelectedWorkspaceId}
							/>
							<CreateWorkspaceDialog
								open={isCreateWorkspaceOpen}
								onOpenChange={setIsCreateWorkspaceOpen}
								onCreateWorkspace={createWorkspace}
								isSubmitting={isCreatingWorkspace}
							/>
							<SessionWorkbench
								workspaceId={selectedWorkspace.id}
								workspaceName={selectedWorkspace.name}
								workspaceBranch={selectedWorkspace.branch}
								workspacePath={selectedWorkspacePath}
								selectedProviderLabel={selectedProvider?.label ?? null}
								selectedProviderId={selectedProviderId}
								providerChoices={providerChoices}
								sessionSnapshot={sessionSnapshot}
								sessionEvents={sessionEvents}
								onSelectProvider={setSelectedProviderId}
								onStartSession={handleStartSession}
								onSubmitPrompt={handleSubmitPrompt}
								onResumeSession={handleResumeSession}
								onAbortSession={handleAbortSession}
								onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
							/>
						</div>
					</section>

					{!inspectorCollapsed && (
						<>
							<ResizeSeparator
								side="right"
								widthAt={inspectorWidth}
								ariaLabel="Resize inspector sidebar"
								ariaMin={MIN_INSPECTOR_WIDTH}
								ariaMax={MAX_INSPECTOR_WIDTH}
								ariaNow={inspectorWidth}
								isActive={isInspectorResizing}
								onMouseDown={handleResizeStart("inspector")}
								onKeyDown={handleResizeKeyDown("inspector")}
							/>

							<aside
								aria-label="Inspector sidebar"
								className="relative h-full shrink-0 overflow-hidden bg-sidebar has-[[data-tabs-zoomed=true]]:overflow-visible"
								style={{ width: `${inspectorWidth}px` }}
							>
								<WorkspaceInspectorSidebar
									providerCatalog={providerCatalog}
									sessionSnapshot={sessionSnapshot}
									workspaceId={selectedWorkspace.id}
									workspaceName={selectedWorkspace.name}
									workspaceBranch={selectedWorkspace.branch}
									workspacePath={selectedWorkspacePath}
									selectedProviderLabel={selectedProvider?.label ?? null}
									sessionState={sessionSnapshot?.state ?? "idle"}
									sessionId={sessionSnapshot?.sessionId ?? null}
								/>
							</aside>
						</>
					)}
				</div>
			</main>
			<Toaster theme={theme} position="bottom-right" visibleToasts={6} />
		</>
	);
}
