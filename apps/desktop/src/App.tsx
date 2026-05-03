import {
	useCallback,
	useEffect,
	useMemo,
	useState,
	type KeyboardEventHandler,
	type MouseEventHandler,
} from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Toaster } from "sonner";
import type { CoreEvent, WorkspaceSessionSummary } from "@dcc/contracts";
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
import {
	workspaceSessionSnapshotFromSummary,
	workspaceSessionsQueryOptions,
} from "./features/sessions/workspace-sessions-query";
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
	getProviderUnhealthyReason,
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
import type { WorkspaceGitPreviewSelection } from "./features/inspector/workspace-git-file-preview";
import {
	buildPlanImplementationPrompt,
	buildPlanImplementationThreadTitle,
} from "./features/panel/plan-content";

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

function getCoreEventSessionId(event: CoreEvent): string | null {
	if ("sessionStarted" in event && event.sessionStarted) {
		return event.sessionStarted.session_id;
	}
	if ("sessionCompleted" in event && event.sessionCompleted) {
		return event.sessionCompleted.session_id;
	}
	if ("sessionAborted" in event && event.sessionAborted) {
		return event.sessionAborted.session_id;
	}
	if ("sessionResumed" in event && event.sessionResumed) {
		return event.sessionResumed.session_id;
	}
	if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
		return event.sessionTurnStarted.session_id;
	}
	if ("sessionTurnDelta" in event && event.sessionTurnDelta) {
		return event.sessionTurnDelta.session_id;
	}
	if ("sessionTurnReasoningStarted" in event && event.sessionTurnReasoningStarted) {
		return event.sessionTurnReasoningStarted.session_id;
	}
	if ("sessionTurnReasoningDelta" in event && event.sessionTurnReasoningDelta) {
		return event.sessionTurnReasoningDelta.session_id;
	}
	if ("sessionTurnReasoningCompleted" in event && event.sessionTurnReasoningCompleted) {
		return event.sessionTurnReasoningCompleted.session_id;
	}
	if ("sessionTurnToolCallStarted" in event && event.sessionTurnToolCallStarted) {
		return event.sessionTurnToolCallStarted.session_id;
	}
	if ("sessionTurnToolCallDelta" in event && event.sessionTurnToolCallDelta) {
		return event.sessionTurnToolCallDelta.session_id;
	}
	if ("sessionTurnToolCallCompleted" in event && event.sessionTurnToolCallCompleted) {
		return event.sessionTurnToolCallCompleted.session_id;
	}
	if ("sessionTurnToolCallFailed" in event && event.sessionTurnToolCallFailed) {
		return event.sessionTurnToolCallFailed.session_id;
	}
	if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
		return event.sessionTurnCompleted.session_id;
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return event.sessionTurnAborted.session_id;
	}
	if ("sessionCheckpointCreated" in event && event.sessionCheckpointCreated) {
		return event.sessionCheckpointCreated.session_id;
	}
	return null;
}

function applyCoreEventToSnapshot(
	snapshot: RuntimeSessionSnapshot,
	event: CoreEvent,
): RuntimeSessionSnapshot {
	if (getCoreEventSessionId(event) !== snapshot.sessionId) {
		return snapshot;
	}

	if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
		return { ...snapshot, activeTurnId: null, lastTurnState: "completed" };
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return { ...snapshot, activeTurnId: null, lastTurnState: "aborted" };
	}
	if ("sessionAborted" in event && event.sessionAborted) {
		return { ...snapshot, state: "aborted", activeTurnId: null };
	}
	if ("sessionResumed" in event && event.sessionResumed) {
		return { ...snapshot, state: "active" };
	}
	if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
		return {
			...snapshot,
			activeTurnId: event.sessionTurnStarted.turn_id,
			lastTurnPrompt: event.sessionTurnStarted.prompt,
			lastTurnState: "running",
		};
	}
	if ("sessionCompleted" in event && event.sessionCompleted) {
		return { ...snapshot, state: "completed", activeTurnId: null };
	}
	return snapshot;
}

export default function App() {
	const { t } = useTranslation("common");
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
		setInspectorCollapsed,
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
	const queryClient = useQueryClient();
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
	const workspaceSessionsQuery = useQuery(
		workspaceSessionsQueryOptions(selectedWorkspace?.id ?? null),
	);
	const workspaceSessions = workspaceSessionsQuery.data ?? [];
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
	const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
	const [inspectorTab, setInspectorTab] = useState<"activity" | "context" | "plan">(
		"activity",
	);
	const [sessionSnapshotsById, setSessionSnapshotsById] = useState<
		Record<string, RuntimeSessionSnapshot>
	>({});
	const [pendingPrompt, setPendingPrompt] = useState<string | null>(null);
	const [pendingPromptSessionId, setPendingPromptSessionId] = useState<
		string | null
	>(null);
	const [editorSelection, setEditorSelection] =
		useState<WorkspaceGitPreviewSelection | null>(null);
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
	const selectedProviderBlockReason = useMemo(
		() => getProviderUnhealthyReason(selectedProvider),
		[selectedProvider],
	);
	useDockUnreadBadge(allWorkspaces);
	const effectiveSelectedSessionId =
		selectedSessionId ?? workspaceSessions[0]?.session.id ?? null;
	const selectedSessionSummary = useMemo(
		() =>
			workspaceSessions.find(
				(session) => session.session.id === effectiveSelectedSessionId,
			) ??
			null,
		[effectiveSelectedSessionId, workspaceSessions],
	);
	const selectedSessionSnapshot = useMemo(() => {
		if (!effectiveSelectedSessionId) {
			return null;
		}

		return (
			sessionSnapshotsById[effectiveSelectedSessionId] ??
			(selectedSessionSummary
				? workspaceSessionSnapshotFromSummary(selectedSessionSummary)
				: null)
		);
	}, [effectiveSelectedSessionId, selectedSessionSummary, sessionSnapshotsById]);
	const openPlanSidebar = useCallback(() => {
		setInspectorCollapsed(false);
		setInspectorTab("plan");
	}, [setInspectorCollapsed]);

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
		const sessionId = selectedSessionSnapshot?.sessionId;
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

		const sp = selectedSessionSnapshot.providerId;
		const sm = selectedSessionSnapshot.model;
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
		selectedSessionSnapshot?.sessionId,
		selectedSessionSnapshot?.providerId,
		selectedSessionSnapshot?.model,
	]);

	useEffect(() => {
		const sessionId = selectedSessionSnapshot?.sessionId;
		if (!sessionId || !selectedProviderId || !selectedModelId) {
			return;
		}
		setSessionComposerSelection(sessionId, {
			providerId: selectedProviderId,
			modelId: selectedModelId,
		});
	}, [selectedSessionSnapshot?.sessionId, selectedProviderId, selectedModelId]);

	useEffect(() => {
		setSelectedSessionId(null);
		setSessionSnapshotsById({});
		setPendingPrompt(null);
		setPendingPromptSessionId(null);
		setEditorSelection(null);
	}, [selectedWorkspace?.id]);

	useEffect(() => {
		if (!selectedWorkspace?.id) {
			return;
		}

		if (workspaceSessions.length === 0) {
			setSelectedSessionId(null);
			return;
		}

		setSessionSnapshotsById((current) => {
			const next = { ...current };
			for (const summary of workspaceSessions) {
				next[summary.session.id] = workspaceSessionSnapshotFromSummary(summary);
			}
			return next;
		});

		setSelectedSessionId((current) => {
			if (current && workspaceSessions.some((session) => session.session.id === current)) {
				return current;
			}

			return workspaceSessions[0]?.session.id ?? null;
		});
	}, [selectedWorkspace?.id, workspaceSessions]);

	/** Keep the selected session snapshot in sync with live stream events from the same session. */
	useEffect(() => {
		const last = sessionEvents[sessionEvents.length - 1];
		if (!last || !effectiveSelectedSessionId) {
			return;
		}

		const eventSessionId = getCoreEventSessionId(last);
		if (eventSessionId !== effectiveSelectedSessionId) {
			return;
		}

		setSessionSnapshotsById((current) => {
			const prev = current[effectiveSelectedSessionId];
			if (!prev) {
				return current;
			}

			const next = applyCoreEventToSnapshot(prev, last);
			if (next === prev) {
				return current;
			}

			return {
				...current,
				[effectiveSelectedSessionId]: next,
			};
		});
	}, [effectiveSelectedSessionId, sessionEvents]);

	const handleStartSession = useCallback(async () => {
		if (!selectedProvider || !selectedWorkspace) {
			return;
		}
		if (selectedProviderBlockReason) {
			toast.error(selectedProviderBlockReason);
			return;
		}

		try {
			const result = await startThread({
				workspaceId: selectedWorkspace.id,
				projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
				providerId: selectedProvider.id,
				model: selectedModel?.id ?? null,
				title: `${selectedWorkspace.name} session`,
			});

			const snapshot: RuntimeSessionSnapshot = {
				sessionId: result.session.id,
				projectId: result.session.projectId,
				workspaceId: result.session.workspaceId,
				providerId: result.session.providerId,
				model: result.session.model,
				state: result.projection.state,
				turnCount: result.projection.turnCount,
				checkpointCount: result.projection.checkpointCount,
				activeTurnId: result.projection.activeTurnId ?? null,
				lastTurnPrompt: null,
				lastTurnState: result.projection.activeTurnId ? "running" : null,
			};
			setSessionSnapshotsById((current) => ({
				...current,
				[result.session.id]: snapshot,
			}));
			setSelectedSessionId(result.session.id);
			queryClient.setQueryData<WorkspaceSessionSummary[]>(
				["workspaceSessions", selectedWorkspace.id],
				(current = []) => {
					const nextSummary: WorkspaceSessionSummary = {
						session: result.session,
						thread: result.thread,
						projection: result.projection,
						lastTurnPrompt: null,
						lastTurnState: result.projection.activeTurnId ? "running" : null,
					};
					return [
						nextSummary,
						...current.filter(
							(summary) => summary.session.id !== result.session.id,
						),
					];
				},
			);
		} catch (error) {
			const message =
				error instanceof Error
					? error.message
					: typeof error === "string"
						? error
						: "Failed to create chat";
			console.error("[dcc] create chat failed:", error);
			toast.error(message);
		}
	}, [
		selectedModel,
		selectedProvider,
		selectedProviderBlockReason,
		selectedWorkspace,
		queryClient,
	]);

	const handleImplementPlanInNewThread = useCallback(
		async (input: { planMarkdown: string; planTitle: string | null }) => {
			const planMarkdown = input.planMarkdown.trim();
			if (!planMarkdown) {
				return;
			}
			if (!selectedProvider || !selectedWorkspace) {
				return;
			}
			if (selectedProviderBlockReason) {
				toast.error(selectedProviderBlockReason);
				return;
			}

			const prompt = buildPlanImplementationPrompt(planMarkdown);
			const threadTitle = buildPlanImplementationThreadTitle(
				planMarkdown,
				input.planTitle,
			);
			let startedSessionId: string | null = null;

			try {
				const started = await startThread({
					workspaceId: selectedWorkspace.id,
					projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
					providerId: selectedProvider.id,
					model: selectedModel?.id ?? null,
					title: threadTitle,
				});
				const sessionId = started.session.id;
				startedSessionId = sessionId;
				openPlanSidebar();

				const startedSnapshot: RuntimeSessionSnapshot = {
					sessionId: started.session.id,
					projectId: started.session.projectId,
					workspaceId: started.session.workspaceId,
					providerId: started.session.providerId,
					model: started.session.model,
					state: started.projection.state,
					turnCount: started.projection.turnCount,
					checkpointCount: started.projection.checkpointCount,
					activeTurnId: started.projection.activeTurnId ?? null,
					lastTurnPrompt: null,
					lastTurnState: started.projection.activeTurnId ? "running" : null,
				};
				setSessionSnapshotsById((current) => ({
					...current,
					[sessionId]: startedSnapshot,
				}));
				setSelectedSessionId(startedSessionId);
				queryClient.setQueryData<WorkspaceSessionSummary[]>(
					["workspaceSessions", selectedWorkspace.id],
					(current = []) => {
						const nextSummary: WorkspaceSessionSummary = {
							session: started.session,
							thread: started.thread,
							projection: started.projection,
							lastTurnPrompt: null,
							lastTurnState: started.projection.activeTurnId ? "running" : null,
						};
						return [
							nextSummary,
						...current.filter((summary) => summary.session.id !== sessionId),
					];
				},
				);

				setPendingPrompt(prompt);
				setPendingPromptSessionId(startedSessionId);
				const result = await sendTurn({
					sessionId,
					prompt,
					providerId: selectedProvider.id,
					model: selectedModel?.id ?? null,
					planMode: false,
					effort: "balanced",
					fastMode: true,
				});

				const resultSnapshot: RuntimeSessionSnapshot = {
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
				};
				setSessionSnapshotsById((current) => ({
					...current,
					[result.session.id]: resultSnapshot,
				}));
				queryClient.setQueryData<WorkspaceSessionSummary[]>(
					["workspaceSessions", result.session.workspaceId],
					(current = []) =>
						current.map((summary) =>
							summary.session.id === result.session.id
								? {
										...summary,
										session: result.session,
										projection: result.projection,
										lastTurnPrompt: result.turn.content,
										lastTurnState: result.turn.state,
									}
								: summary,
						),
				);
			} catch (error) {
				const message =
					error instanceof Error
						? error.message
						: typeof error === "string"
							? error
							: "Failed to create implementation thread";
				console.error("[dcc] implement plan thread failed:", error);
				toast.error(message);
			} finally {
				setPendingPrompt((current) => (current === prompt ? null : current));
				setPendingPromptSessionId((current) =>
					current === startedSessionId ? null : current,
				);
			}
		},
		[
			openPlanSidebar,
			queryClient,
			selectedModel,
			selectedProvider,
			selectedProviderBlockReason,
			selectedWorkspace,
		],
	);

	const handleSubmitPrompt = useCallback(async (turn: ComposerSubmittedTurn) => {
		const trimmedPrompt = turn.rawPrompt.trim();
		if (trimmedPrompt.length === 0) {
			return;
		}
		if (selectedProviderBlockReason) {
			toast.error(selectedProviderBlockReason);
			return;
		}

		let currentSession = selectedSessionSnapshot;
		let currentSessionId = selectedSessionId;

		try {
			if (!currentSession || !currentSessionId) {
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
					lastTurnPrompt: null,
					lastTurnState: started.projection.activeTurnId ? "running" : null,
				};
				const startedSessionId = started.session.id;
				currentSessionId = startedSessionId;
				setSelectedSessionId(currentSessionId);
				const startedSnapshot = currentSession as RuntimeSessionSnapshot;
				setSessionSnapshotsById((current) => ({
					...current,
					[startedSessionId]: startedSnapshot,
				}));
				queryClient.setQueryData<WorkspaceSessionSummary[]>(
					["workspaceSessions", selectedWorkspace.id],
					(current = []) => {
						const nextSummary: WorkspaceSessionSummary = {
							session: started.session,
							thread: started.thread,
							projection: started.projection,
							lastTurnPrompt: null,
							lastTurnState: started.projection.activeTurnId ? "running" : null,
						};
						return [
							nextSummary,
							...current.filter(
								(summary) => summary.session.id !== started.session.id,
							),
						];
					},
				);
			}

			if (!currentSessionId || !currentSession) {
				return;
			}

			setPendingPrompt(trimmedPrompt);
			setPendingPromptSessionId(currentSessionId);

			const result = await sendTurn({
				sessionId: currentSessionId,
				prompt: trimmedPrompt,
				providerId: selectedProvider?.id ?? null,
				model: selectedModel?.id ?? null,
				planMode: turn.envelope.planMode,
				effort: turn.envelope.effort,
				fastMode: turn.envelope.fastMode,
			});

			const resultSnapshot: RuntimeSessionSnapshot = {
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
			};
			setSessionSnapshotsById((current) => ({
				...current,
				[result.session.id]: resultSnapshot,
			}));
			queryClient.setQueryData<WorkspaceSessionSummary[]>(
				["workspaceSessions", result.session.workspaceId],
				(current = []) =>
					current.map((summary) =>
						summary.session.id === result.session.id
							? {
									...summary,
									session: result.session,
									projection: result.projection,
									lastTurnPrompt: result.turn.content,
									lastTurnState: result.turn.state,
								}
							: summary,
					),
			);
		} catch (error) {
			const message =
				error instanceof Error
					? error.message
					: typeof error === "string"
						? error
						: "Failed to send prompt";
			console.error("[dcc] send prompt failed:", error);
			toast.error(message);
		} finally {
			setPendingPrompt((current) =>
				currentSessionId && current === trimmedPrompt ? null : current,
			);
			setPendingPromptSessionId((current) =>
				current === currentSessionId ? null : current,
			);
		}
	}, [
		queryClient,
		selectedModel,
		selectedProvider,
		selectedProviderBlockReason,
		selectedSessionId,
		selectedSessionSnapshot,
		selectedWorkspace,
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

	const handleSelectSession = useCallback((sessionId: string) => {
		setSelectedSessionId(sessionId);
	}, []);

	const handleResumeSession = useCallback(async () => {
		if (!selectedSessionSnapshot || !canResumeSession(selectedSessionSnapshot)) {
			return;
		}

		const result = await resumeSession({ sessionId: selectedSessionSnapshot.sessionId });
		setSessionSnapshotsById((current) => {
			const prev = current[selectedSessionSnapshot.sessionId];
			if (!prev) {
				return current;
			}

			return {
				...current,
				[selectedSessionSnapshot.sessionId]: {
					...prev,
					state: result.projection.state,
					turnCount: result.projection.turnCount,
					checkpointCount: result.projection.checkpointCount,
					activeTurnId: result.projection.activeTurnId ?? null,
				},
			};
		});
		queryClient.setQueryData<WorkspaceSessionSummary[]>(
			["workspaceSessions", selectedSessionSnapshot.workspaceId],
			(current = []) =>
				current.map((summary) =>
					summary.session.id === selectedSessionSnapshot.sessionId
						? {
								...summary,
								session: result.session,
								projection: result.projection,
							}
						: summary,
				),
		);
	}, [queryClient, selectedSessionSnapshot]);

	const handleOpenEditorFile = useCallback(
		(selection: WorkspaceGitPreviewSelection | null) => {
			setEditorSelection(selection);
		},
		[],
	);

	const handleCloseEditor = useCallback(() => {
		setEditorSelection(null);
	}, []);

	const handleAbortSession = useCallback(async () => {
		const visiblePendingPrompt =
			pendingPromptSessionId === effectiveSelectedSessionId ? pendingPrompt : null;
		if (
			!selectedSessionSnapshot ||
			!canAbortRun(selectedSessionSnapshot, visiblePendingPrompt)
		) {
			return;
		}

		const result = await abortRun({
			sessionId: selectedSessionSnapshot.sessionId,
			reason: "Stopped from shell",
		});
		setSessionSnapshotsById((current) => {
			const prev = current[selectedSessionSnapshot.sessionId];
			if (!prev) {
				return current;
			}

			return {
				...current,
				[selectedSessionSnapshot.sessionId]: {
					...prev,
					state: result.projection.state,
					turnCount: result.projection.turnCount,
					checkpointCount: result.projection.checkpointCount,
					activeTurnId: result.projection.activeTurnId ?? null,
				},
			};
		});
		queryClient.setQueryData<WorkspaceSessionSummary[]>(
			["workspaceSessions", selectedSessionSnapshot.workspaceId],
			(current = []) =>
				current.map((summary) =>
					summary.session.id === selectedSessionSnapshot.sessionId
						? {
								...summary,
								session: result.session,
								projection: result.projection,
							}
						: summary,
				),
		);
		setPendingPrompt((current) =>
			visiblePendingPrompt && current === visiblePendingPrompt ? null : current,
		);
		setPendingPromptSessionId((current) =>
			current === selectedSessionSnapshot.sessionId ? null : current,
		);
	}, [
		effectiveSelectedSessionId,
		pendingPrompt,
		pendingPromptSessionId,
		queryClient,
		selectedSessionSnapshot,
	]);

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

	const visiblePendingPrompt =
		effectiveSelectedSessionId === pendingPromptSessionId ? pendingPrompt : null;
	const sidebarRailWidth = sidebarCollapsed ? 76 : sidebarWidth;
	const hasWorkspace = Boolean(selectedWorkspace);

	return (
		<>
			<main
				aria-label={t("app.shellAria")}
				className="relative h-screen overflow-hidden bg-background font-sans text-foreground antialiased"
			>
				<div className="relative flex h-full min-h-0 bg-background">
					<aside
						aria-label={t("app.workspaceSidebarAria")}
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
						ariaLabel={t("app.resizeSidebarAria")}
						ariaMin={MIN_SIDEBAR_WIDTH}
						ariaMax={MAX_SIDEBAR_WIDTH}
						ariaNow={sidebarWidth}
						isActive={isSidebarResizing}
						onMouseDown={handleResizeStart("sidebar")}
						onKeyDown={handleResizeKeyDown("sidebar")}
					/>

					<section
						aria-label={t("app.workspacePanelAria")}
						className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-background"
					>
						{/* Keep z-index below workspace viewport so header/toolbar clicks reach buttons (Tauri drag region steals first click when on top). */}
						<div
							aria-label={t("app.workspaceDragRegionAria")}
							data-tauri-drag-region
							className="absolute inset-x-0 top-0 z-10 h-9 bg-transparent"
						/>
						<div
							aria-label={t("app.workspaceViewportAria")}
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
									sessions={workspaceSessions}
									selectedSessionId={selectedSessionId}
									isLoadingSessions={workspaceSessionsQuery.isPending}
									sessionSnapshot={selectedSessionSnapshot}
									sessionEvents={sessionEvents}
									pendingPrompt={visiblePendingPrompt}
									onSelectProvider={handleSelectProvider}
									onSelectModel={handleSelectModel}
									onStartSession={handleStartSession}
									onSelectSession={handleSelectSession}
									onSubmitPrompt={handleSubmitPrompt}
									onResumeSession={handleResumeSession}
									onAbortSession={handleAbortSession}
									updateInfo={appUpdateInfo}
									isInstallingUpdate={isInstallingUpdate}
								onInstallUpdate={installUpdate}
								editorSelection={editorSelection}
								onCloseEditor={handleCloseEditor}
								onOpenPlanSidebar={openPlanSidebar}
								onImplementPlanInNewThread={handleImplementPlanInNewThread}
							/>
							) : (
								<WorkspaceBootstrapState
									selectedProviderLabel={selectedProvider?.label ?? null}
									selectedModelLabel={selectedModel?.label ?? null}
									onCreateWorkspace={() => openWorkspaceDialog("open")}
									onCloneWorkspace={() => openWorkspaceDialog("clone")}
								/>
							)}
						</div>
					</section>

					{!inspectorCollapsed && (
						<>
							<ResizeSeparator
								side="right"
								widthAt={inspectorWidth}
								ariaLabel={t("app.resizeInspectorAria")}
								ariaMin={MIN_INSPECTOR_WIDTH}
								ariaMax={MAX_INSPECTOR_WIDTH}
								ariaNow={inspectorWidth}
								isActive={isInspectorResizing}
								onMouseDown={handleResizeStart("inspector")}
								onKeyDown={handleResizeKeyDown("inspector")}
							/>

							<aside
								aria-label={t("app.inspectorSidebarAria")}
								className="relative h-full shrink-0 overflow-hidden bg-sidebar"
								style={{ width: `${inspectorWidth}px` }}
							>
							<WorkspaceInspectorSidebar
								providerCatalog={providerCatalog}
								sessionSnapshot={selectedSessionSnapshot}
								sessionEvents={sessionEvents}
								workspaceId={selectedWorkspace?.id ?? null}
								workspaceName={selectedWorkspace?.name ?? null}
								workspaceBranch={selectedWorkspace?.branch ?? null}
								workspacePath={selectedWorkspacePath}
								selectedProviderLabel={selectedProvider?.label ?? null}
								selectedModelLabel={selectedModel?.label ?? null}
								sessionState={selectedSessionSnapshot?.state ?? "idle"}
								sessionId={selectedSessionSnapshot?.sessionId ?? null}
								selectedPreview={editorSelection}
								onSelectPreview={handleOpenEditorFile}
								activeTab={inspectorTab}
								onTabChange={setInspectorTab}
							/>
							</aside>
						</>
					)}
				</div>
			</main>
				<SettingsDialog
					open={isSettingsOpen}
					onOpenChange={setIsSettingsOpen}
					onOpenShortcuts={() => setIsShortcutSheetOpen(true)}
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
