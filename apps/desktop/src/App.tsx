import { useCallback, useEffect, useMemo, useState } from "react";
import { cn } from "@/lib/utils";
import {
	MAX_INSPECTOR_WIDTH,
	MAX_SIDEBAR_WIDTH,
	MIN_INSPECTOR_WIDTH,
	MIN_SIDEBAR_WIDTH,
	SIDEBAR_RESIZE_HIT_AREA,
} from "./shell/layout";
import { useShellPanels } from "./shell/use-panels";
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

export default function App() {
	useZoom(1);

	const {
		handleResizeKeyDown,
		handleResizeStart,
		inspectorWidth,
		isInspectorResizing,
		isSidebarResizing,
		sidebarCollapsed,
		sidebarWidth,
		setSidebarCollapsed,
	} = useShellPanels();
	const {
		allWorkspaces,
		createWorkspace,
		filter,
		filteredWorkspaces,
		isCreatingWorkspace,
		selectedWorkspace,
		selectedWorkspaceId,
		setFilter,
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
	const [sessionDraft, setSessionDraft] = useState(
		"Describe the change you want in this workspace. The Rust core and Tauri bridge stay the boundary.",
	);
	const [sessionSnapshot, setSessionSnapshot] =
		useState<RuntimeSessionSnapshot | null>(null);
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
		setSessionDraft(
			"Describe the change you want in this workspace. The Rust core and Tauri bridge stay the boundary.",
		);
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

	const handleSendTurn = useCallback(async () => {
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
			prompt: sessionDraft.trim(),
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
		setSessionDraft("");
	}, [
		selectedProvider,
		selectedWorkspace.id,
		selectedWorkspace.name,
		selectedWorkspace.projectId,
		sessionDraft,
		sessionSnapshot,
	]);

	const handleResumeSession = useCallback(async () => {
		if (!sessionSnapshot) {
			return;
		}

		const result = await resumeSession({ sessionId: sessionSnapshot.sessionId });
		setSessionSnapshot((current) =>
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
		setSessionSnapshot((current) =>
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
		<main className="relative h-screen overflow-hidden bg-background font-sans text-foreground antialiased">
			<div className="dcc-app-shell relative flex h-full min-h-0 min-w-0 bg-background">
				<aside
					className="dcc-sidebar dcc-workbench-rail relative flex h-full shrink-0 flex-col overflow-hidden border-r border-border bg-sidebar"
					style={{ width: `${sidebarRailWidth}px` }}
					aria-label="Workspace sidebar"
				>
					<WorkspacesSidebar
						collapsed={sidebarCollapsed}
						filter={filter}
						isCreatingWorkspace={isCreatingWorkspace}
						onFilterChange={setFilter}
						onSelectWorkspace={setSelectedWorkspaceId}
						onCreateWorkspace={() => setIsCreateWorkspaceOpen(true)}
						onToggleCollapsed={() => setSidebarCollapsed((value) => !value)}
						selectedWorkspaceId={selectedWorkspaceId}
						workspaces={filteredWorkspaces}
					/>
				</aside>

				<div
					role="separator"
					tabIndex={0}
					aria-label="Resize sidebar"
					aria-orientation="vertical"
					aria-valuemin={MIN_SIDEBAR_WIDTH}
					aria-valuemax={MAX_SIDEBAR_WIDTH}
					aria-valuenow={sidebarWidth}
					onMouseDown={handleResizeStart("sidebar")}
					onKeyDown={handleResizeKeyDown("sidebar")}
					className="group absolute inset-y-0 z-30 cursor-ew-resize touch-none outline-none"
					style={{
						left: `${sidebarRailWidth - SIDEBAR_RESIZE_HIT_AREA / 2}px`,
						width: `${SIDEBAR_RESIZE_HIT_AREA}px`,
					}}
				>
					<span
						aria-hidden="true"
						className={cn(
							"pointer-events-none absolute inset-y-0 left-1/2 -translate-x-1/2 transition-[width,background-color,box-shadow]",
							isSidebarResizing
								? "w-[2px] bg-foreground/80 shadow-[0_0_12px_rgba(0,0,0,0.12)] dark:shadow-[0_0_12px_rgba(255,255,255,0.16)]"
								: "w-px bg-border group-hover:w-[2px] group-hover:bg-muted-foreground/75 group-focus-visible:w-[2px] group-focus-visible:bg-muted-foreground/75",
						)}
					/>
				</div>

				<section
					className="dcc-main dcc-main--workbench relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background"
					aria-label="Workspace panel"
				>
					<div
						className="flex min-h-0 min-w-0 flex-1 flex-col bg-background"
						aria-label="Workspace viewport"
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
							sessionDraft={sessionDraft}
							onSessionDraftChange={setSessionDraft}
							onSelectProvider={setSelectedProviderId}
							onStartSession={handleStartSession}
							onSendTurn={handleSendTurn}
							onResumeSession={handleResumeSession}
							onAbortSession={handleAbortSession}
							onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
						/>
					</div>
				</section>

				<div
					role="separator"
					tabIndex={0}
					aria-label="Resize inspector sidebar"
					aria-orientation="vertical"
					aria-valuemin={MIN_INSPECTOR_WIDTH}
					aria-valuemax={MAX_INSPECTOR_WIDTH}
					aria-valuenow={inspectorWidth}
					onMouseDown={handleResizeStart("inspector")}
					onKeyDown={handleResizeKeyDown("inspector")}
					className="group absolute inset-y-0 z-30 cursor-ew-resize touch-none outline-none"
					style={{
						right: `${Math.max(0, inspectorWidth - SIDEBAR_RESIZE_HIT_AREA)}px`,
						width: `${SIDEBAR_RESIZE_HIT_AREA}px`,
					}}
				>
					<span
						aria-hidden="true"
						className={cn(
							"pointer-events-none absolute inset-y-0 left-0 transition-[width,background-color,box-shadow]",
							isInspectorResizing
								? "w-[2px] bg-transparent shadow-none"
								: "w-px bg-border group-hover:w-[2px] group-hover:bg-muted-foreground/75 group-focus-visible:w-[2px] group-focus-visible:bg-muted-foreground/75",
						)}
					/>
				</div>

				<aside
					className="dcc-inspector-aside relative h-full shrink-0 overflow-hidden border-l border-border bg-sidebar"
					style={{ width: `${inspectorWidth}px` }}
					aria-label="Inspector sidebar"
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
			</div>
		</main>
	);
}
