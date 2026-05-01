import {
	useCallback,
	useEffect,
	useMemo,
	useState,
	type KeyboardEventHandler,
	type MouseEventHandler,
} from "react";
import { useQuery } from "@tanstack/react-query";
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
import { SettingsDialog } from "./features/settings";
import { OnboardingWizard } from "./features/onboarding";
import { ShortcutCheatsheetDialog } from "./features/shortcuts";
import { useDockUnreadBadge } from "./features/dock-badge/useDockUnreadBadge";
import { useAppUpdate } from "./features/updater";
import {
	SessionWorkbench,
	type RuntimeSessionSnapshot,
} from "./features/sessions/session-workbench";
import { WorkspaceBootstrapState } from "./features/panel/WorkspaceBootstrapState";
import { useSessionEventFeed } from "./features/sessions/use-session-event-feed";
import { FALLBACK_PROVIDER_CATALOG } from "./lib/fallback-provider-catalog";
import { listProviders } from "./lib/provider-api";
import { listWorkspaces } from "./lib/workspace-api";
import {
	abortRun,
	resumeSession,
	sendTurn,
	startThread,
} from "./lib/session-api";
import { useAppearance } from "./components/theme-provider";
import {
	SELECTED_PROVIDER_STORAGE_KEY,
	SELECTED_MODEL_STORAGE_KEY,
	getSessionComposerSelection,
	resolveSelectedProviderId,
	resolveSelectedModelId,
	setSessionComposerSelection,
} from "./features/providers/provider-selection.logic";
import type { ComposerSubmittedTurn } from "./features/composer/composer-turn";
import { workspaceToSummary } from "./features/workspaces/use-workspaces";
import {
	canAbortRun,
	canResumeSession,
} from "./features/sessions/session-chrome-state";

const ONBOARDING_COMPLETE_KEY = "dcc.onboarding.complete";

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
	const workspacesQuery = useQuery({
		queryKey: ["workspaces"],
		queryFn: async () => {
			const result = await listWorkspaces();
			return result.workspaces.map(workspaceToSummary);
		},
		staleTime: 60_000,
		refetchOnWindowFocus: false,
	});
	const workspacesFromBackend = workspacesQuery.data ?? [];
	const {
		allWorkspaces,
		createWorkspace,
		cloneWorkspaceFromUrl,
		filteredWorkspaces,
		isCreatingWorkspace,
		selectedWorkspace,
		selectedWorkspaceId,
		setSelectedWorkspaceId,
	} = useWorkspacesPanel(workspacesFromBackend);
	const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);
	const [isCreateWorkspaceOpen, setIsCreateWorkspaceOpen] = useState(false);
	const [workspaceCreationMode, setWorkspaceCreationMode] = useState<"open" | "clone">(
		"open",
	);
	const [isSettingsOpen, setIsSettingsOpen] = useState(false);
	const [isOnboardingOpen, setIsOnboardingOpen] = useState(() => {
		if (typeof window === "undefined") {
			return false;
		}

		if (window.location.search.includes("onboarding=1")) {
			return true;
		}

		return window.localStorage.getItem(ONBOARDING_COMPLETE_KEY) !== "true";
	});
	const [isShortcutSheetOpen, setIsShortcutSheetOpen] = useState(false);
	const { events: sessionEvents } = useSessionEventFeed();
	const providersQuery = useQuery({
		queryKey: ["providers", "catalog"],
		queryFn: listProviders,
		staleTime: 300_000,
		placeholderData: () => ({ catalog: FALLBACK_PROVIDER_CATALOG }),
	});
	const providerCatalog =
		providersQuery.data?.catalog ?? FALLBACK_PROVIDER_CATALOG;
	const [selectedProviderId, setSelectedProviderId] = useState<string | null>(() => {
		if (typeof window === "undefined") {
			return null;
		}

		return window.localStorage.getItem(SELECTED_PROVIDER_STORAGE_KEY);
	});
	const [selectedModelId, setSelectedModelId] = useState<string | null>(() => {
		if (typeof window === "undefined") {
			return null;
		}

		return window.localStorage.getItem(SELECTED_MODEL_STORAGE_KEY);
	});
	const [sessionSnapshot, setSessionSnapshot] =
		useState<RuntimeSessionSnapshot | null>(null);
	const [pendingPrompt, setPendingPrompt] = useState<string | null>(null);
	const { theme, setTheme } = useAppearance();
	const {
		update: appUpdateInfo,
		isInstalling: isInstallingUpdate,
		installUpdate,
	} = useAppUpdate();
	const providerChoices = providerCatalog.providers;
	const selectedWorkspacePath =
		selectedWorkspace?.worktreePath ?? selectedWorkspace?.rootPath ?? null;
	const selectedProvider = useMemo(
		() =>
			providerChoices.find((provider) => provider.id === selectedProviderId) ??
			providerChoices[0] ??
			null,
		[providerChoices, selectedProviderId],
	);
	const selectedModel = useMemo(
		() =>
			selectedProvider?.models.find((model) => model.id === selectedModelId) ??
			selectedProvider?.models.find((model) => model.recommended) ??
			selectedProvider?.models[0] ??
			null,
		[selectedModelId, selectedProvider],
	);
	useDockUnreadBadge(allWorkspaces);

	useEffect(() => {
		if (providerChoices.length === 0) {
			return;
		}

		setSelectedProviderId((current) => {
			return resolveSelectedProviderId(providerChoices, current);
		});
	}, [providerChoices]);

	useEffect(() => {
		if (selectedProvider) {
			setSelectedModelId((current) =>
				resolveSelectedModelId(selectedProvider, current),
			);
		}
	}, [selectedProvider]);

	useEffect(() => {
		if (typeof window === "undefined") {
			return;
		}

		if (selectedProviderId) {
			window.localStorage.setItem(
				SELECTED_PROVIDER_STORAGE_KEY,
				selectedProviderId,
			);
			return;
		}

		window.localStorage.removeItem(SELECTED_PROVIDER_STORAGE_KEY);
	}, [selectedProviderId]);

	useEffect(() => {
		if (typeof window === "undefined") {
			return;
		}

		if (selectedModelId) {
			window.localStorage.setItem(SELECTED_MODEL_STORAGE_KEY, selectedModelId);
			return;
		}

		window.localStorage.removeItem(SELECTED_MODEL_STORAGE_KEY);
	}, [selectedModelId]);

	/** Helmor-style: restore provider/model for this session, else follow backend snapshot. */
	useEffect(() => {
		if (providerChoices.length === 0) {
			return;
		}
		const sessionId = sessionSnapshot?.sessionId;
		if (!sessionId) {
			return;
		}

		const stored = getSessionComposerSelection(sessionId);
		if (stored) {
			const provider = providerChoices.find((p) => p.id === stored.providerId);
			const model = provider?.models.find((m) => m.id === stored.modelId);
			if (provider && model) {
				setSelectedProviderId(stored.providerId);
				setSelectedModelId(stored.modelId);
				return;
			}
		}

		const sp = sessionSnapshot.providerId;
		const sm = sessionSnapshot.model;
		if (sp && sm) {
			const provider = providerChoices.find((p) => p.id === sp);
			const model = provider?.models.find((m) => m.id === sm);
			if (provider && model) {
				setSelectedProviderId(sp);
				setSelectedModelId(sm);
			}
		}
	}, [
		providerChoices,
		sessionSnapshot?.sessionId,
		sessionSnapshot?.providerId,
		sessionSnapshot?.model,
	]);

	useEffect(() => {
		const sessionId = sessionSnapshot?.sessionId;
		if (!sessionId || !selectedProviderId || !selectedModelId) {
			return;
		}
		setSessionComposerSelection(sessionId, {
			providerId: selectedProviderId,
			modelId: selectedModelId,
		});
	}, [sessionSnapshot?.sessionId, selectedProviderId, selectedModelId]);

	useEffect(() => {
		setSessionSnapshot(null);
		setPendingPrompt(null);
	}, [selectedWorkspace?.id]);

	/** Keep `activeTurnId` in sync with live stream (turn finished / new turn / abort) after the send returns. */
	useEffect(() => {
		const last = sessionEvents[sessionEvents.length - 1];
		if (!last) {
			return;
		}

		setSessionSnapshot((prev) => {
			if (!prev) {
				return prev;
			}
			const sid = prev.sessionId;

			if ("sessionTurnCompleted" in last && last.sessionTurnCompleted?.session_id === sid) {
				return { ...prev, activeTurnId: null, lastTurnState: "completed" };
			}
			if ("sessionTurnAborted" in last && last.sessionTurnAborted?.session_id === sid) {
				return { ...prev, activeTurnId: null, lastTurnState: "aborted" };
			}
			if ("sessionAborted" in last && last.sessionAborted?.session_id === sid) {
				return { ...prev, state: "aborted", activeTurnId: null };
			}
			if ("sessionResumed" in last && last.sessionResumed?.session_id === sid) {
				return { ...prev, state: "active" };
			}
			if ("sessionTurnStarted" in last && last.sessionTurnStarted?.session_id === sid) {
				const started = last.sessionTurnStarted;
				return {
					...prev,
					activeTurnId: started?.turn_id ?? null,
					lastTurnState: "running",
				};
			}
			if ("sessionCompleted" in last && last.sessionCompleted?.session_id === sid) {
				return { ...prev, state: "completed", activeTurnId: null };
			}
			return prev;
		});
	}, [sessionEvents]);

	const handleStartSession = useCallback(async () => {
		if (!selectedProvider || !selectedWorkspace) {
			return;
		}

		const result = await startThread({
			workspaceId: selectedWorkspace.id,
			projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
			providerId: selectedProvider.id,
			model: selectedModel?.id ?? null,
			title: `${selectedWorkspace.name} session`,
		});

		setSessionSnapshot({
			sessionId: result.session.id,
			projectId: result.session.projectId,
			workspaceId: result.session.workspaceId,
			providerId: result.session.providerId,
			model: result.session.model,
			state: result.projection.state,
			turnCount: result.projection.turnCount,
			checkpointCount: result.projection.checkpointCount,
			activeTurnId: result.projection.activeTurnId ?? null,
		});
	}, [selectedModel, selectedProvider, selectedWorkspace]);

	const handleSubmitPrompt = useCallback(async (turn: ComposerSubmittedTurn) => {
		const trimmedPrompt = turn.rawPrompt.trim();
		if (trimmedPrompt.length === 0) {
			return;
		}

		let currentSession = sessionSnapshot;

		try {
			if (!currentSession) {
				if (!selectedProvider || !selectedWorkspace) {
					return;
				}

				const started = await startThread({
					workspaceId: selectedWorkspace.id,
					projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
					providerId: selectedProvider.id,
					model: selectedModel?.id ?? null,
					title: `${selectedWorkspace.name} session`,
				});

				currentSession = {
					sessionId: started.session.id,
					projectId: started.session.projectId,
					workspaceId: started.session.workspaceId,
					providerId: started.session.providerId,
					model: started.session.model,
					state: started.projection.state,
					turnCount: started.projection.turnCount,
					checkpointCount: started.projection.checkpointCount,
					activeTurnId: started.projection.activeTurnId ?? null,
				};
				setSessionSnapshot(currentSession);
			}

			setPendingPrompt(trimmedPrompt);

			const result = await sendTurn({
				sessionId: currentSession.sessionId,
				prompt: trimmedPrompt,
				providerId: selectedProvider?.id ?? null,
				model: selectedModel?.id ?? null,
				planMode: turn.envelope.planMode,
				effort: turn.envelope.effort,
				fastMode: turn.envelope.fastMode,
			});

			setSessionSnapshot({
				sessionId: result.session.id,
				projectId: result.session.projectId,
				workspaceId: result.session.workspaceId,
				providerId: result.session.providerId,
				model: result.session.model,
				state: result.projection.state,
				turnCount: result.projection.turnCount,
				checkpointCount: result.projection.checkpointCount,
				activeTurnId: result.projection.activeTurnId ?? null,
				lastTurnPrompt: result.turn.content,
				lastTurnState: result.turn.state,
			});
		} finally {
			setPendingPrompt(null);
		}
	}, [
		selectedModel,
		selectedProvider,
		selectedWorkspace,
		sessionSnapshot,
	]);

	const handleSelectProvider = useCallback(
		(providerId: string) => {
			setSelectedProviderId(providerId);
			const provider = providerChoices.find((candidate) => candidate.id === providerId);
			setSelectedModelId(resolveSelectedModelId(provider ?? null, null));
		},
		[providerChoices],
	);

	const handleSelectModel = useCallback(
		(modelId: string) => {
			const owning = providerChoices.find((provider) =>
				provider.models.some((model) => model.id === modelId),
			);
			if (owning) {
				setSelectedProviderId(owning.id);
			}
			setSelectedModelId(modelId);
		},
		[providerChoices],
	);

	const handleResumeSession = useCallback(async () => {
		if (!sessionSnapshot || !canResumeSession(sessionSnapshot)) {
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
						activeTurnId: result.projection.activeTurnId ?? null,
					}
				: current,
		);
	}, [sessionSnapshot]);

	const handleAbortSession = useCallback(async () => {
		if (!sessionSnapshot || !canAbortRun(sessionSnapshot, pendingPrompt)) {
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
						activeTurnId: result.projection.activeTurnId ?? null,
					}
				: current,
		);
	}, [pendingPrompt, sessionSnapshot]);

	const handleCompleteOnboarding = useCallback(() => {
		try {
			window.localStorage.setItem(ONBOARDING_COMPLETE_KEY, "true");
		} catch {
			/* localStorage unavailable */
		}
		setIsOnboardingOpen(false);
	}, []);

	const openWorkspaceDialog = useCallback((mode: "open" | "clone") => {
		setWorkspaceCreationMode(mode);
		setIsCreateWorkspaceOpen(true);
	}, []);

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
	const hasWorkspace = Boolean(selectedWorkspace);

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
							onCreateWorkspace={() => openWorkspaceDialog("open")}
							onCloneWorkspace={() => openWorkspaceDialog("clone")}
							onOpenSettings={() => setIsSettingsOpen(true)}
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
						{/* Keep z-index below workspace viewport so header/toolbar clicks reach buttons (Tauri drag region steals first click when on top). */}
						<div
							aria-label="Workspace panel drag region"
							data-tauri-drag-region
							className="absolute inset-x-0 top-0 z-10 h-9 bg-transparent"
						/>
						<div
							aria-label="Workspace viewport"
							className="relative z-20 flex min-h-0 flex-1 flex-col bg-background"
						>
							<WorkspaceCommandPalette
								open={isCommandPaletteOpen}
								onOpenChange={setIsCommandPaletteOpen}
								workspaces={allWorkspaces}
								selectedWorkspaceId={selectedWorkspaceId}
								onSelectWorkspace={setSelectedWorkspaceId}
								onCreateWorkspace={() => openWorkspaceDialog("open")}
								onCloneWorkspace={() => openWorkspaceDialog("clone")}
								onOpenSettings={() => setIsSettingsOpen(true)}
								onOpenOnboarding={() => setIsOnboardingOpen(true)}
								onOpenShortcuts={() => setIsShortcutSheetOpen(true)}
							/>
							<CreateWorkspaceDialog
								open={isCreateWorkspaceOpen}
								mode={workspaceCreationMode}
								onOpenChange={setIsCreateWorkspaceOpen}
								onCreateWorkspace={createWorkspace}
								onCloneWorkspace={cloneWorkspaceFromUrl}
								isSubmitting={isCreatingWorkspace}
							/>
							{hasWorkspace && selectedWorkspace ? (
								<SessionWorkbench
									workspaceId={selectedWorkspace.id}
									workspaceName={selectedWorkspace.name}
									workspaceBranch={selectedWorkspace.branch}
									workspacePath={selectedWorkspacePath}
									selectedProviderLabel={selectedProvider?.label ?? null}
									selectedModelLabel={selectedModel?.label ?? null}
									selectedProviderId={selectedProviderId}
									selectedModelId={selectedModelId}
									providerChoices={providerChoices}
									sessionSnapshot={sessionSnapshot}
									sessionEvents={sessionEvents}
									pendingPrompt={pendingPrompt}
									onSelectProvider={handleSelectProvider}
									onSelectModel={handleSelectModel}
									onStartSession={handleStartSession}
									onSubmitPrompt={handleSubmitPrompt}
									onResumeSession={handleResumeSession}
									onAbortSession={handleAbortSession}
									onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
									updateInfo={appUpdateInfo}
									isInstallingUpdate={isInstallingUpdate}
									onInstallUpdate={installUpdate}
								/>
							) : (
								<WorkspaceBootstrapState
									selectedProviderLabel={selectedProvider?.label ?? null}
									selectedModelLabel={selectedModel?.label ?? null}
									onCreateWorkspace={() => openWorkspaceDialog("open")}
									onCloneWorkspace={() => openWorkspaceDialog("clone")}
									onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
								/>
							)}
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
									sessionEvents={sessionEvents}
									workspaceId={selectedWorkspace?.id ?? null}
									workspaceName={selectedWorkspace?.name ?? null}
									workspaceBranch={selectedWorkspace?.branch ?? null}
									workspacePath={selectedWorkspacePath}
									selectedProviderLabel={selectedProvider?.label ?? null}
									selectedModelLabel={selectedModel?.label ?? null}
									sessionState={sessionSnapshot?.state ?? "idle"}
									sessionId={sessionSnapshot?.sessionId ?? null}
								/>
							</aside>
						</>
					)}
				</div>
			</main>
				<SettingsDialog
					open={isSettingsOpen}
					onOpenChange={setIsSettingsOpen}
					theme={theme}
					onThemeChange={setTheme}
					providerCatalog={providerCatalog}
					selectedProviderId={selectedProviderId}
					selectedModelId={selectedModelId}
					onSelectProvider={handleSelectProvider}
					onSelectModel={handleSelectModel}
				/>
			<ShortcutCheatsheetDialog
				open={isShortcutSheetOpen}
				onOpenChange={setIsShortcutSheetOpen}
			/>
			<OnboardingWizard
				open={isOnboardingOpen}
				onOpenChange={setIsOnboardingOpen}
				onComplete={handleCompleteOnboarding}
			/>
			<Toaster theme={theme} position="bottom-right" visibleToasts={6} />
		</>
	);
}
