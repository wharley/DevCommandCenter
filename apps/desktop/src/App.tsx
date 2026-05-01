import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import { SIDEBAR_RESIZE_HIT_AREA } from "./shell/layout";
import { useShellPanels } from "./shell/use-panels";
import { useZoom } from "./shell/use-zoom";
import {
	WorkspacesSidebar,
	WorkspaceCommandPalette,
	CreateWorkspaceDialog,
	useWorkspacesPanel,
} from "./features/workspaces";
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
		showArchived,
		setFilter,
		setSelectedWorkspaceId,
		setShowArchived,
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
		"Bring the Helmor shell density into Dev Command Center, but keep the Tauri + Rust boundary clean.",
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
			"Bring the Helmor shell density into Dev Command Center, but keep the Tauri + Rust boundary clean.",
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

	const visibleWidth = sidebarCollapsed ? 76 : sidebarWidth;
	const shellStyle = {
		"--dcc-sidebar-width": `${visibleWidth}px`,
	} as CSSProperties;

	return (
		<div className="dcc-shell" style={shellStyle}>
			<aside className="dcc-sidebar">
				<WorkspacesSidebar
					collapsed={sidebarCollapsed}
					filter={filter}
					onFilterChange={setFilter}
					onSelectWorkspace={setSelectedWorkspaceId}
					onShowArchivedChange={setShowArchived}
					onCreateWorkspace={() => setIsCreateWorkspaceOpen(true)}
					onToggleCollapsed={() => setSidebarCollapsed((value) => !value)}
					selectedWorkspaceId={selectedWorkspaceId}
					sidebarWidth={visibleWidth}
					showArchived={showArchived}
					workspaces={filteredWorkspaces}
				/>
			</aside>

			<div
				className="dcc-shell__divider"
				role="separator"
				aria-orientation="vertical"
				aria-label="Resize sidebar"
				tabIndex={0}
				style={{ width: `${SIDEBAR_RESIZE_HIT_AREA}px` }}
				onMouseDown={handleResizeStart("sidebar")}
				onKeyDown={handleResizeKeyDown("sidebar")}
			>
				<div className="dcc-shell__divider-hit" />
			</div>

		<main className="dcc-main">
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
					providerCatalog={providerCatalog}
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
			</main>
		</div>
	);
}
