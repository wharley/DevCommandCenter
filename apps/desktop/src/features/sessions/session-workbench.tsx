import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
	AlertTriangleIcon,
	CheckCircle2Icon,
	ExternalLinkIcon,
	GitPullRequestArrowIcon,
	LoaderCircleIcon,
	MinusCircleIcon,
} from "lucide-react";
import type {
	CoreEvent,
	ProviderCatalog,
	ProviderRuntimeConfig,
	WorkspaceSetupReport,
} from "@dcc/contracts";
import {
	WorkspaceTerminalDrawer,
	type TerminalAgentContext,
} from "@/features/terminal";
import {
	addTerminal as addTerminalTab,
	ensureTerminal as ensureTerminalTab,
	setActiveTerminal as setActiveTerminalTab,
	getTerminalRuntimeId,
	renameTerminal as renameTerminalTab,
} from "@/features/terminal/terminal-tabs-store";
import {
	attachTerminal,
	detachTerminal,
	ensureTerminal as ensureTerminalRuntime,
	writeTerminalInput,
	type TerminalListener,
} from "@/features/terminal/terminal-store";
import { WorkspacePanel } from "@/features/panel";
import type { WorkspaceSurfaceSelection } from "@/features/panel/workspace-surface";
import type {
	ComposerDelegationRequest,
	ComposerSubmittedTurn,
} from "@/features/composer/composer-turn";
import type {
	AgentResolutionRunRequest,
	AgentResolutionRunResult,
} from "@/features/merge/agent-conflict-resolution";
import type { RuntimeSessionSnapshot } from "./workbench-types";
import type { ManualDelegationRequest } from "./delegation-request";
import type { AgentInitiatedDelegationRequest } from "./agent-delegation-request";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import type {
	OpenTerminalRequest,
	TerminalScopeTarget,
} from "@/features/terminal/terminal-scope";
import {
	getWorkspaceTerminalUiState,
	updateWorkspaceTerminalUiState,
	type WorkspaceTerminalUiStateUpdate,
	type WorkspaceTerminalUiStates,
} from "@/features/terminal/terminal-workspace-ui-state";
import {
	dispatchWorkbenchCommand,
	subscribeWorkbenchCommand,
} from "@/features/workspaces/workbench-command";
import { recordUxMetric } from "@/lib/ux-metrics";
import { openExternal } from "@/lib/shell-api";
import {
	workspaceRecordSetupOutcome,
	workspaceSkipSetup,
} from "@/lib/workspace-api";
import { pathBasename } from "@/lib/path-basename";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import type { WorkspaceFileReference } from "@/components/workspace-file-reference";
import {
	Dialog,
	DialogClose,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import type {
	MultiWorkspaceDeliveryCommitReview,
	MultiWorkspaceDeliveryPreview,
	MultiWorkspaceDeliveryResult,
} from "@/features/workspaces/multi-workspace-delivery";

export type { RuntimeSessionSnapshot } from "./workbench-types";

type SessionWorkbenchProps = {
	workspaceId: string;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
	workspaceSetupReport?: WorkspaceSetupReport | null;
	/** Active worktree/member id used to isolate terminal state and runtime ownership. */
	terminalWorkspaceId?: string | null;
	/** Active worktree/member labels used by the terminal runtime context. */
	terminalWorkspaceName?: string | null;
	terminalWorkspaceBranch?: string | null;
	terminalProjectBranch?: string | null;
	/** Project id — terminals are scoped per project. */
	projectId?: string | null;
	/** Project root (`rootPath`) — terminals open here, outside the worktree. */
	terminalRootPath?: string | null;
	/** Optional user-facing project identity; falls back to the technical root name. */
	projectLabel?: string | null;
	projectIcon?: string | null;
	projectColor?: string | null;
	/** Active mission worktree path. Unlike workspacePath, this does not fall back to rootPath. */
	terminalWorktreePath?: string | null;
	workspaceScopeOptions?: Array<{
		id: string;
		name: string;
		branch: string;
		hasChanges: boolean | null;
		needsDelivery: boolean | null;
		icon?: string | null;
		color?: string | null;
	}>;
	selectedWorkspaceScopeId?: string | null;
	onSelectWorkspaceScope?: (workspaceId: string) => void;
	onPrepareWorkspaceScopeDelivery?: () => Promise<MultiWorkspaceDeliveryPreview[]>;
	onDeliverWorkspaceScope?: (
		workspaceIds: string[],
		commitReviews: MultiWorkspaceDeliveryCommitReview[],
	) => Promise<MultiWorkspaceDeliveryResult[]>;
	sessionQueryScope?: string;
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	selectedProviderId: string | null;
	selectedModelId: string | null;
	selectedProviderRuntime: ProviderRuntimeConfig | null;
	providerChoices: ProviderCatalog["providers"];
	sessions: WorkspaceSessionSummary[];
	selectedSessionId: string | null;
	isLoadingSessions: boolean;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	sessionEvents: CoreEvent[];
	pendingPrompt: string | null;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
	onStartSession: () => void;
	onSelectSession: (sessionId: string) => void;
	onCloseSession: (sessionId: string) => void;
	onRestoreSession: (sessionId: string) => void;
	onOpenSessionSearch: () => void;
	onSubmitPrompt: (
		turn: ComposerSubmittedTurn,
		options?: { forceNewSession?: boolean; targetSessionId?: string | null },
	) => Promise<void>;
	onSteerPrompt: (turn: ComposerSubmittedTurn) => Promise<void>;
	onQueuePrompt: (turn: ComposerSubmittedTurn) => Promise<void>;
	onResumeSession: () => Promise<void>;
	onAbortSession: () => void;
	onDelegate: (request: ManualDelegationRequest) => Promise<void>;
	onDelegatePrompt: (request: ComposerDelegationRequest) => Promise<void>;
	onAgentDelegate: (request: AgentInitiatedDelegationRequest) => Promise<void>;
	sessionActionSessionId: string | null;
	surfaceSelection: WorkspaceSurfaceSelection | null;
	surfaceSelectionWorkspaceId: string | null;
	fileSurfaceTransitionRequestId?: number;
	onFileSurfaceTransitionConfirmed?: () => void;
	onFileSurfaceClosed?: () => void;
	onCloseSurface: () => void;
	onOpenPlanSurface: () => void;
	onOpenFileReference?: (reference: WorkspaceFileReference) => void;
	onImplementPlanInNewThread: (input: {
		planMarkdown: string;
		planTitle: string | null;
	}) => Promise<boolean>;
	composerPrefill?: {
		text: string;
		nonce: number;
		mode?: "append" | "replace";
	} | null;
	onComposerPrefillConsumed?: (prefill: { text: string; nonce: number }) => void;
	composerFocusRequestKey?: number | null;
	/** Current inspector visibility — picks the open vs. close affordance. */
	inspectorCollapsed?: boolean;
	onInspectorCollapsedChange?: (collapsed: boolean) => void;
	/** Toggles the inspector open/closed — wired to the header control. */
	onToggleInspector?: () => void;
	/** Reveals the inspector to review the current Git changes. */
	onReviewChanges?: () => void;
	onCompleteWorkspace?: (workspaceId: string) => Promise<void> | void;
	onCreateTaskFromBranch?: (branch: string) => Promise<void>;
	/** Opens the inspector and previews an implementation delegation diff. */
	onReviewDelegation?: (delegationId: string) => void;
	onRerunDelegation?: (input: {
		delegationId: string;
		targetProviderId: string;
	}) => Promise<void>;
	onResolveConflictWithAgent: (
		request: AgentResolutionRunRequest,
	) => Promise<AgentResolutionRunResult>;
	onOpenAgentSession: (sessionId: string) => void;
	onMergeConflictStateChanged: (workspaceRoot: string) => Promise<void> | void;
	/** Increment to open the Delegate dialog from outside (command palette). */
	delegateSignal?: number;
};

export function SessionWorkbench({
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	workspaceSetupReport = null,
	terminalWorkspaceId,
	terminalWorkspaceName,
	terminalWorkspaceBranch,
	terminalProjectBranch,
	projectId,
	terminalRootPath,
	projectLabel,
	projectIcon,
	projectColor,
	terminalWorktreePath,
	workspaceScopeOptions = [],
	selectedWorkspaceScopeId = null,
	onSelectWorkspaceScope,
	onPrepareWorkspaceScopeDelivery,
	onDeliverWorkspaceScope,
	sessionQueryScope = "local",
	selectedProviderLabel,
	selectedModelLabel,
	selectedProviderId,
	selectedModelId,
	selectedProviderRuntime,
	providerChoices,
	sessions,
	selectedSessionId,
	isLoadingSessions,
	sessionSnapshot,
	sessionEvents,
	pendingPrompt,
	onSelectProvider,
	onSelectModel,
	onStartSession,
	onSelectSession,
	onCloseSession,
	onRestoreSession,
	onOpenSessionSearch,
	onSubmitPrompt,
	onSteerPrompt,
	onQueuePrompt,
	onResumeSession,
	onAbortSession,
	onDelegate,
	onDelegatePrompt,
	onAgentDelegate,
	sessionActionSessionId,
	surfaceSelection,
	surfaceSelectionWorkspaceId,
	fileSurfaceTransitionRequestId,
	onFileSurfaceTransitionConfirmed,
	onFileSurfaceClosed,
	onCloseSurface,
	onOpenPlanSurface,
	onOpenFileReference,
	onImplementPlanInNewThread,
	composerPrefill,
	onComposerPrefillConsumed,
	composerFocusRequestKey = null,
	inspectorCollapsed,
	onInspectorCollapsedChange,
	onToggleInspector,
	onReviewChanges,
	onCompleteWorkspace,
	onCreateTaskFromBranch,
	onReviewDelegation,
	onRerunDelegation,
	onResolveConflictWithAgent,
	onOpenAgentSession,
	onMergeConflictStateChanged,
	delegateSignal,
}: SessionWorkbenchProps) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [terminalUiStates, setTerminalUiStates] =
		useState<WorkspaceTerminalUiStates>({});
	const [terminalComposerPrefill, setTerminalComposerPrefill] = useState<{
		workspaceId: string;
		text: string;
		nonce: number;
		mode: "append";
	} | null>(null);
	const terminalPrefillNonceRef = useRef(0);
	const handleComposerPrefillConsumed = useCallback(
		(prefill: { text: string; nonce: number }) => {
			if (
				terminalComposerPrefill?.nonce === prefill.nonce &&
				terminalComposerPrefill.text === prefill.text
			) {
				setTerminalComposerPrefill(null);
				return;
			}
			onComposerPrefillConsumed?.(prefill);
		},
		[onComposerPrefillConsumed, terminalComposerPrefill],
	);
	useEffect(() => {
		// Terminal context belongs to the workspace where it was collected and
		// must not leak into the next workspace's composer.
		setTerminalComposerPrefill(null);
	}, [workspaceId]);
	const inspectorBeforeTerminalExpandRef = useRef<boolean | null>(null);
	const [deliveryOpen, setDeliveryOpen] = useState(false);
	const [deliveryPreparing, setDeliveryPreparing] = useState(false);
	const [deliveryRunning, setDeliveryRunning] = useState(false);
	const [deliveryPreviews, setDeliveryPreviews] = useState<
		MultiWorkspaceDeliveryPreview[] | null
	>(null);
	const [deliveryResults, setDeliveryResults] = useState<
		MultiWorkspaceDeliveryResult[] | null
	>(null);
	const [deliveryError, setDeliveryError] = useState<string | null>(null);
	const deliveryPreparationIdRef = useRef(0);
	const sessionState = sessionSnapshot?.state ?? "idle";
	const resolvedProjectLabel =
		projectLabel?.trim() ||
		(terminalRootPath ? pathBasename(terminalRootPath) : (projectId ?? workspaceName));
	const sessionId = sessionSnapshot?.sessionId ?? null;
	const terminalWorkspaceKey = terminalWorkspaceId ?? workspaceId;
	const terminalUiState = getWorkspaceTerminalUiState(
		terminalUiStates,
		terminalWorkspaceKey,
	);
	const terminalOpen = terminalUiState.open;
	const terminalExpanded = terminalUiState.expanded;
	const terminalScopeKind = terminalUiState.scopeKind;
	const updateTerminalUiState = useCallback(
		(update: WorkspaceTerminalUiStateUpdate) => {
			setTerminalUiStates((current) =>
				updateWorkspaceTerminalUiState(
					current,
					terminalWorkspaceKey,
					update,
				),
			);
		},
		[terminalWorkspaceKey],
	);
	const terminalProjectKey = projectId ?? terminalWorkspaceKey;
	const terminalScopes: TerminalScopeTarget[] = useMemo(
		() => [
			{
				kind: "worktree",
				label: t("terminalDock.scopes.worktree"),
				scopeKey: `worktree:${terminalWorkspaceKey}`,
				cwd: terminalWorktreePath ?? null,
				projectLabel: resolvedProjectLabel,
				branchLabel: terminalWorkspaceBranch ?? workspaceBranch,
				protected: true,
				disabledReason: terminalWorktreePath
					? null
					: t("terminalDock.scopes.noWorktreePath"),
			},
			{
				kind: "project",
				label: t("terminalDock.scopes.project"),
				scopeKey: terminalProjectKey,
				cwd: terminalRootPath ?? workspacePath,
				projectLabel: resolvedProjectLabel,
				branchLabel: terminalProjectBranch ?? null,
				protected: false,
				disabledReason:
					terminalRootPath ?? workspacePath
						? null
						: t("terminalDock.scopes.noProjectPath"),
			},
		],
		[
			t,
			terminalProjectKey,
			terminalProjectBranch,
			terminalRootPath,
			terminalWorktreePath,
			terminalWorkspaceKey,
			terminalWorkspaceBranch,
			resolvedProjectLabel,
			workspaceBranch,
			workspacePath,
		],
	);
	const activeTerminalScope =
		terminalScopes.find((scope) => scope.kind === terminalScopeKind && scope.cwd) ??
		terminalScopes.find((scope) => scope.cwd) ??
		terminalScopes[0];

	const handleOpenTerminal = useCallback(
		(request: OpenTerminalRequest = { scope: "worktree" }) => {
			const scope =
				terminalScopes.find((item) => item.kind === request.scope) ??
				terminalScopes[0];
			if (!scope.cwd) {
				return;
			}
			recordUxMetric("terminal_discovered");

			if (request.terminalId) {
				setActiveTerminalTab(scope.scopeKey, request.terminalId);
			} else if (request.createNew) {
				addTerminalTab(scope.scopeKey);
			} else {
				ensureTerminalTab(scope.scopeKey);
			}
			updateTerminalUiState((current) => ({
				...current,
				open: true,
				scopeKind: scope.kind,
			}));
		},
		[terminalScopes, updateTerminalUiState],
	);
	const refreshWorkspaceSetupState = useCallback(async () => {
		await Promise.all([
			queryClient.invalidateQueries({
				queryKey: ["workspaces", sessionQueryScope],
			}),
			queryClient.invalidateQueries({
				queryKey: ["workspaceBundles", sessionQueryScope],
			}),
		]);
	}, [queryClient, sessionQueryScope]);
	const handleRunRecommendedSetup = useCallback(
		async (commands: string[]) => {
			const scope = terminalScopes.find(
				(item) => item.kind === "worktree" && Boolean(item.cwd),
			);
			if (!scope?.cwd || commands.length === 0) {
				throw new Error(t("composer.executionDock.setup.unavailable"));
			}
			// Setup must never be typed into an existing terminal that may already be busy.
			const terminalId = addTerminalTab(scope.scopeKey, {
				reuseAtCapacity: false,
			});
			if (!terminalId) {
				throw new Error(t("composer.executionDock.setup.unavailable"));
			}
			renameTerminalTab(
				scope.scopeKey,
				terminalId,
				t("composer.executionDock.setup.terminalTitle"),
			);
			updateTerminalUiState((current) => ({
				...current,
				open: true,
				expanded: false,
				scopeKind: "worktree",
			}));
			const runtimeId = getTerminalRuntimeId(scope.scopeKey, terminalId);
			const snapshot = await ensureTerminalRuntime(runtimeId, scope.cwd, {
				title: "Setup",
				workspaceName: terminalWorkspaceName ?? workspaceName,
				workspaceBranch: terminalWorkspaceBranch ?? workspaceBranch,
				providerLabel: selectedProviderLabel,
				sessionState,
				sessionId,
			});
			if (snapshot.status !== "running") {
				throw new Error(t("composer.executionDock.setup.unavailable"));
			}

			await new Promise<void>((resolve, reject) => {
				const marker = `__DCC_SETUP_DONE_${crypto.randomUUID()}__`;
				let tail = "";
				let settled = false;
				const finish = (success: boolean) => {
					if (settled) return;
					settled = true;
					detachTerminal(runtimeId, listener);
					void workspaceRecordSetupOutcome({
						workspaceRoot: scope.cwd!,
						success,
					})
						.then(refreshWorkspaceSetupState)
						.then(() => {
							if (success) resolve();
							else reject(new Error(t("composer.executionDock.setup.commandFailed")));
						})
						.catch(reject);
				};
				const listener: TerminalListener = {
					onChunk(data) {
						tail = `${tail}${data}`.slice(-8192);
						const match = tail.match(new RegExp(`${marker}:(\\d+)`));
						if (match) finish(match[1] === "0");
					},
					onStatusChange(status) {
						if (status === "exited" || status === "error") finish(false);
					},
				};
				attachTerminal(runtimeId, listener);
				const commandChain = commands.map((command) => `( ${command} )`).join(" && ");
				writeTerminalInput(
					runtimeId,
					`{ ${commandChain}; }; __dcc_setup_status=$?; printf '\\n${marker}:%s\\n' "$__dcc_setup_status"\r`,
				);
			});
		},
		[
			refreshWorkspaceSetupState,
			selectedProviderLabel,
			sessionId,
			sessionState,
			t,
			terminalScopes,
			terminalWorkspaceBranch,
			terminalWorkspaceName,
			updateTerminalUiState,
			workspaceBranch,
			workspaceName,
		],
	);
	const handleSkipRecommendedSetup = useCallback(async () => {
		const workspaceRoot = terminalWorktreePath ?? workspacePath;
		if (!workspaceRoot) {
			throw new Error(t("composer.executionDock.setup.unavailable"));
		}
		await workspaceSkipSetup({ workspaceRoot });
		await refreshWorkspaceSetupState();
	}, [refreshWorkspaceSetupState, t, terminalWorktreePath, workspacePath]);
	const restoreInspectorAfterTerminal = useCallback(() => {
		if (inspectorBeforeTerminalExpandRef.current === false) {
			onInspectorCollapsedChange?.(false);
		}
		inspectorBeforeTerminalExpandRef.current = null;
	}, [onInspectorCollapsedChange]);
	const handleTerminalOpenChange = useCallback(
		(next: boolean) => {
			updateTerminalUiState((current) => ({
				...current,
				open: next,
				expanded: next ? current.expanded : false,
			}));
			if (!next) {
				restoreInspectorAfterTerminal();
				requestAnimationFrame(() => dispatchWorkbenchCommand("composer.focus"));
			}
		},
		[restoreInspectorAfterTerminal, updateTerminalUiState],
	);
	const handleTerminalExpandedChange = useCallback(
		(expanded: boolean) => {
			if (expanded) {
				if (inspectorBeforeTerminalExpandRef.current === null) {
					inspectorBeforeTerminalExpandRef.current = inspectorCollapsed ?? true;
				}
				if (inspectorCollapsed === false) {
					onInspectorCollapsedChange?.(true);
				}
			} else {
				restoreInspectorAfterTerminal();
			}
			updateTerminalUiState((current) => ({ ...current, expanded }));
		},
		[
			inspectorCollapsed,
			onInspectorCollapsedChange,
			restoreInspectorAfterTerminal,
			updateTerminalUiState,
		],
	);
	const handleSendTerminalToAgent = useCallback(
		(context: TerminalAgentContext) => {
			terminalPrefillNonceRef.current += 1;
			const branch = context.branchLabel ? ` · ${context.branchLabel}` : "";
			const safeContent = context.content.replaceAll(
				"</terminal_output>",
				"&lt;/terminal_output&gt;",
			);
			const source = context.selectionOnly
				? t("terminalDock.agentContext.selection")
				: t("terminalDock.agentContext.recentOutput");
			setTerminalComposerPrefill({
				workspaceId,
				nonce: terminalPrefillNonceRef.current,
				mode: "append",
				text: [
					t("terminalDock.agentContext.prompt", { source }),
					"",
					"<terminal_output>",
					`project: ${context.projectLabel}`,
					`scope: ${context.scopeLabel}${branch}`,
					`cwd: ${context.cwd}`,
					"---",
					safeContent,
					"</terminal_output>",
				].join("\n"),
			});
			handleTerminalOpenChange(false);
		},
		[handleTerminalOpenChange, t],
	);

	useEffect(
		() => () => {
			restoreInspectorAfterTerminal();
		},
		[restoreInspectorAfterTerminal, terminalWorkspaceKey],
	);

	useEffect(
		() =>
			subscribeWorkbenchCommand((command) => {
				if (command === "terminal.toggle") {
					if (terminalOpen) {
						handleTerminalOpenChange(false);
					} else {
						handleOpenTerminal({ scope: "worktree" });
					}
				} else if (command === "terminal.openWorktree") {
					handleOpenTerminal({ scope: "worktree" });
				} else if (command === "terminal.openProject") {
					handleOpenTerminal({ scope: "project" });
				} else if (command === "terminal.newWorktree") {
					handleOpenTerminal({ scope: "worktree", createNew: true });
				}
			}),
		[handleOpenTerminal, handleTerminalOpenChange, terminalOpen],
	);

	// Full-bleed terminal takeover — hide the chat column entirely.
	const chatHidden = terminalOpen && terminalExpanded;
	const deliverableScopeOptions = workspaceScopeOptions.filter(
		(workspace) => workspace.needsDelivery === true,
	);
	const scopeChangesLoading = workspaceScopeOptions.some(
		(workspace) => workspace.needsDelivery === null,
	);
	const handleOpenDelivery = useCallback(() => {
		const preparationId = deliveryPreparationIdRef.current + 1;
		deliveryPreparationIdRef.current = preparationId;
		setDeliveryPreviews(null);
		setDeliveryResults(null);
		setDeliveryError(null);
		setDeliveryOpen(true);
		if (!onPrepareWorkspaceScopeDelivery) {
			setDeliveryError(t("workspaceScope.delivery.preparationUnavailable"));
			return;
		}
		setDeliveryPreparing(true);
		void onPrepareWorkspaceScopeDelivery()
			.then((previews) => {
				if (deliveryPreparationIdRef.current !== preparationId) return;
				setDeliveryPreviews(previews);
			})
			.catch((error) => {
				if (deliveryPreparationIdRef.current !== preparationId) return;
				setDeliveryError(
					error instanceof Error
						? error.message
						: typeof error === "string"
							? error
							: String(error),
				);
			})
			.finally(() => {
				if (deliveryPreparationIdRef.current === preparationId) {
					setDeliveryPreparing(false);
				}
			});
	}, [onPrepareWorkspaceScopeDelivery, t]);
	const handleDeliver = useCallback(async () => {
		if (!onDeliverWorkspaceScope || !deliveryPreviews) return;
		const commitReviews = deliveryPreviews.flatMap((preview) =>
			preview.commit ? [preview.commit] : [],
		);
		const workspaceIds = deliveryPreviews.map((preview) => preview.workspaceId);
		setDeliveryRunning(true);
		setDeliveryError(null);
		try {
			setDeliveryResults(await onDeliverWorkspaceScope(workspaceIds, commitReviews));
		} catch (error) {
			setDeliveryError(
				error instanceof Error
					? error.message
					: typeof error === "string"
						? error
						: String(error),
			);
		} finally {
			setDeliveryRunning(false);
		}
	}, [deliveryPreviews, onDeliverWorkspaceScope]);
	const deliveryReadyCount =
		deliveryPreviews?.filter(
			(preview) => preview.action !== "blocked" && preview.action !== "no-changes",
		).length ?? 0;
	const hasInvalidCommitReview =
		deliveryPreviews?.some(
			(preview) => preview.commit !== null && !preview.commit.subject.trim(),
		) ?? false;

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			{!chatHidden ? (
				<div className="@container/header-actions flex min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden">
					{workspaceScopeOptions.length > 1 ? (
						<div className="flex h-9 shrink-0 items-center gap-2 border-b border-border/60 bg-muted/20 px-3">
							<span className="text-[11px] font-medium text-muted-foreground">
								{t("workspaceScope.projects")}
							</span>
							<div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
								{workspaceScopeOptions.map((workspace) => {
									const selected = workspace.id === selectedWorkspaceScopeId;
									return (
										<button
											type="button"
											key={workspace.id}
											title={`${workspace.name} · ${workspace.branch}`}
											onClick={() => onSelectWorkspaceScope?.(workspace.id)}
											className={`flex h-6 min-w-0 max-w-52 shrink-0 items-center gap-1.5 rounded-md border px-2 text-[11.5px] font-medium transition-colors ${
												selected
													? "border-border bg-background text-foreground"
													: "border-transparent text-muted-foreground hover:bg-muted/70 hover:text-foreground"
											}`}
										>
											<span
												aria-hidden
												className={`size-1.5 shrink-0 rounded-full ${
													workspace.hasChanges === true
														? "bg-amber-400"
														: workspace.hasChanges === false
															? "bg-emerald-500"
															: "bg-muted-foreground/40"
												}`}
											/>
											<span className="truncate">{workspace.name}</span>
										</button>
									);
								})}
							</div>
							<span className="ml-auto shrink-0 rounded-full bg-cyan-500/15 px-2 py-0.5 text-[10px] font-semibold text-cyan-300">
								{t("workspaceScope.authorizedCount", {
									count: workspaceScopeOptions.length,
								})}
							</span>
							{onDeliverWorkspaceScope ? (
								<Button
									type="button"
									variant="outline"
									size="xs"
									className="gap-1.5"
									disabled={scopeChangesLoading || deliverableScopeOptions.length === 0}
									onClick={handleOpenDelivery}
								>
									{!scopeChangesLoading && deliverableScopeOptions.length === 0 ? (
										<CheckCircle2Icon className="size-3.5 text-emerald-500" />
									) : (
										<GitPullRequestArrowIcon className="size-3.5" />
									)}
									{scopeChangesLoading
										? t("workspaceScope.delivery.checking")
										: deliverableScopeOptions.length === 0
											? t("workspaceScope.delivery.complete")
											: t("workspaceScope.delivery.action", {
													count: deliverableScopeOptions.length,
												})}
								</Button>
							) : null}
						</div>
					) : null}
					<WorkspacePanel
						workspaceId={workspaceId}
						workspaceName={workspaceName}
						workspaceBranch={workspaceBranch}
						workspacePath={workspacePath}
						workspaceSetupReport={workspaceSetupReport}
						projectRootPath={terminalRootPath}
						projectLabel={resolvedProjectLabel}
						projectIcon={projectIcon}
						projectColor={projectColor}
						isIsolatedWorkspace={Boolean(terminalWorktreePath)}
						workspaceContextProjects={workspaceScopeOptions}
						sessionQueryScope={sessionQueryScope}
						selectedProviderId={selectedProviderId}
						selectedModelId={selectedModelId}
						selectedProviderRuntime={selectedProviderRuntime}
						providerChoices={providerChoices}
						sessions={sessions}
						selectedSessionId={selectedSessionId}
						isLoadingSessions={isLoadingSessions}
						sessionSnapshot={sessionSnapshot}
						sessionEvents={sessionEvents}
						pendingPrompt={pendingPrompt}
						onSelectProvider={onSelectProvider}
						onSelectModel={onSelectModel}
						onStartSession={onStartSession}
						onSelectSession={onSelectSession}
						onCloseSession={onCloseSession}
						onRestoreSession={onRestoreSession}
						onOpenSessionSearch={onOpenSessionSearch}
						onSubmitPrompt={onSubmitPrompt}
						onSteerPrompt={onSteerPrompt}
						onQueuePrompt={onQueuePrompt}
						onResumeSession={onResumeSession}
						onAbortSession={onAbortSession}
						onDelegate={onDelegate}
						onDelegatePrompt={onDelegatePrompt}
						onAgentDelegate={onAgentDelegate}
						sessionActionSessionId={sessionActionSessionId}
						surfaceSelection={surfaceSelection}
						surfaceSelectionWorkspaceId={surfaceSelectionWorkspaceId}
						fileSurfaceTransitionRequestId={fileSurfaceTransitionRequestId}
						onFileSurfaceTransitionConfirmed={onFileSurfaceTransitionConfirmed}
						onFileSurfaceClosed={onFileSurfaceClosed}
						onCloseSurface={onCloseSurface}
						onOpenPlanSurface={onOpenPlanSurface}
						onOpenFileReference={onOpenFileReference}
						onImplementPlanInNewThread={onImplementPlanInNewThread}
						terminalScopes={terminalScopes}
						onOpenTerminal={handleOpenTerminal}
						externalComposerPrefill={
							terminalComposerPrefill?.workspaceId === workspaceId
								? terminalComposerPrefill
								: composerPrefill
						}
						onExternalComposerPrefillConsumed={handleComposerPrefillConsumed}
						composerFocusRequestKey={composerFocusRequestKey}
						inspectorCollapsed={inspectorCollapsed}
						onToggleInspector={onToggleInspector}
						onReviewChanges={onReviewChanges}
						onCompleteWorkspace={onCompleteWorkspace}
						onOpenMultiProjectDelivery={handleOpenDelivery}
						onCreateTaskFromBranch={onCreateTaskFromBranch}
						onRunRecommendedSetup={handleRunRecommendedSetup}
						onSkipRecommendedSetup={handleSkipRecommendedSetup}
						onReviewDelegation={onReviewDelegation}
						onRerunDelegation={onRerunDelegation}
						onResolveConflictWithAgent={onResolveConflictWithAgent}
						onOpenAgentSession={onOpenAgentSession}
						onMergeConflictStateChanged={onMergeConflictStateChanged}
						delegateSignal={delegateSignal}
					/>
				</div>
			) : null}

			{terminalOpen ? (
				<WorkspaceTerminalDrawer
					open={terminalOpen}
					onOpenChange={handleTerminalOpenChange}
					expanded={terminalExpanded}
					onExpandedChange={handleTerminalExpandedChange}
					scopeKey={activeTerminalScope.scopeKey}
					scopeLabel={activeTerminalScope.label}
					cwd={activeTerminalScope.cwd}
					scopes={terminalScopes}
					activeScopeKind={activeTerminalScope.kind}
					onScopeChange={(kind) => {
						if (kind !== terminalScopeKind) {
							recordUxMetric("terminal_scope_switched");
						}
						updateTerminalUiState((current) => ({
							...current,
							scopeKind: kind,
						}));
					}}
					workspaceName={terminalWorkspaceName ?? workspaceName}
					workspaceBranch={terminalWorkspaceBranch ?? workspaceBranch}
					providerLabel={selectedProviderLabel}
					sessionState={sessionState}
					sessionId={sessionId}
					onSendToAgent={handleSendTerminalToAgent}
				/>
			) : null}

			<Dialog
				open={deliveryOpen}
				onOpenChange={(open) => {
					if (!deliveryRunning && !deliveryPreparing) setDeliveryOpen(open);
				}}
			>
				<DialogContent
					className="sm:max-w-2xl"
					showCloseButton={!deliveryRunning && !deliveryPreparing}
				>
					<DialogHeader>
						<DialogTitle>{t("workspaceScope.delivery.title")}</DialogTitle>
						<DialogDescription>
							{deliveryResults
								? t("workspaceScope.delivery.resultDescription")
								: deliveryPreparing
									? t("workspaceScope.delivery.preparingDescription")
									: t("workspaceScope.delivery.reviewDescription")}
						</DialogDescription>
					</DialogHeader>

					{deliveryResults ? (
						<div className="max-h-[min(52vh,420px)] space-y-2 overflow-y-auto pr-1">
							{deliveryResults.map((result) => {
								const ResultIcon =
									result.status === "delivered"
										? CheckCircle2Icon
										: result.status === "failed"
											? AlertTriangleIcon
											: MinusCircleIcon;
								return (
									<div
										key={result.workspaceId}
										className="flex items-start gap-2.5 rounded-lg border border-border/70 bg-muted/25 p-3"
									>
										<ResultIcon
											className={`mt-0.5 size-4 shrink-0 ${
												result.status === "delivered"
													? "text-emerald-500"
													: result.status === "failed"
														? "text-destructive"
														: "text-muted-foreground"
											}`}
										/>
										<div className="min-w-0 flex-1">
											<p className="truncate text-sm font-medium">{result.name}</p>
											<p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
												{result.message}
											</p>
											{result.requestUrl ? (
												<Button
													type="button"
													variant="link"
													size="xs"
													className="mt-1 h-auto gap-1 p-0 text-xs"
													onClick={() => void openExternal(result.requestUrl!)}
												>
													{t("workspaceScope.delivery.openRequest")}
													<ExternalLinkIcon className="size-3" />
												</Button>
											) : null}
										</div>
									</div>
								);
							})}
						</div>
					) : deliveryPreparing ? (
						<div
							className="flex min-h-52 items-center justify-center gap-3 rounded-lg border border-border/60 bg-muted/20 px-6 py-8"
							role="status"
							aria-live="polite"
						>
							<LoaderCircleIcon className="size-5 shrink-0 animate-spin text-muted-foreground" />
							<div>
								<p className="text-sm font-medium">
									{t("workspaceScope.delivery.preparing")}
								</p>
								<p className="mt-1 text-xs text-muted-foreground">
									{t("workspaceScope.delivery.preparingHint")}
								</p>
							</div>
						</div>
					) : deliveryPreviews ? (
						<div className="max-h-[min(62vh,560px)] space-y-3 overflow-y-auto pr-1">
							{deliveryPreviews.map((preview, index) => {
								const workspace = workspaceScopeOptions.find(
									(option) => option.id === preview.workspaceId,
								);
								const blocked = preview.action === "blocked";
								return (
									<div
										key={preview.workspaceId}
										className={`rounded-lg border p-3 ${
											blocked
												? "border-destructive/40 bg-destructive/5"
												: "border-border/70 bg-muted/20"
										}`}
									>
										<div className="flex items-start gap-2">
											{blocked ? (
												<AlertTriangleIcon className="mt-0.5 size-4 shrink-0 text-destructive" />
											) : (
												<span className="mt-1.5 size-2 shrink-0 rounded-full bg-amber-400" />
											)}
											<div className="min-w-0 flex-1">
												<div className="flex flex-wrap items-center gap-x-2 gap-y-1">
													<p className="truncate text-sm font-medium">{preview.name}</p>
													{workspace?.branch ? (
														<span className="truncate font-mono text-[11px] text-muted-foreground">
															{workspace.branch}
														</span>
													) : null}
													<span className="ml-auto rounded-full border border-border/70 px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
														{t(`workspaceScope.delivery.actions.${preview.action}`)}
													</span>
												</div>
												{preview.message ? (
													<p className="mt-1 text-xs leading-relaxed text-destructive">
														{preview.message}
													</p>
												) : null}
											</div>
										</div>

										{preview.commit ? (
											<div className="mt-3 grid gap-2 border-t border-border/60 pt-3">
												<label
													className="text-[11px] font-medium"
													htmlFor={`multi-workspace-commit-subject-${index}`}
												>
													{t("workspaceScope.delivery.commitSubject")}
												</label>
												<Textarea
													id={`multi-workspace-commit-subject-${index}`}
													value={preview.commit.subject}
													onChange={(event) => {
														const subject = event.target.value;
														setDeliveryPreviews((current) =>
															current?.map((item) =>
																item.workspaceId === preview.workspaceId && item.commit
																	? { ...item, commit: { ...item.commit, subject } }
																	: item,
															) ?? null,
														);
													}}
													className="min-h-16 resize-y font-mono text-xs"
												/>
												<label
													className="text-[11px] font-medium"
													htmlFor={`multi-workspace-commit-body-${index}`}
												>
													{t("workspaceScope.delivery.commitBody")}
												</label>
												<Textarea
													id={`multi-workspace-commit-body-${index}`}
													value={preview.commit.body ?? ""}
													onChange={(event) => {
														const body = event.target.value;
														setDeliveryPreviews((current) =>
															current?.map((item) =>
																item.workspaceId === preview.workspaceId && item.commit
																	? { ...item, commit: { ...item.commit, body } }
																	: item,
															) ?? null,
														);
													}}
													className="min-h-16 resize-y font-mono text-xs"
												/>
												<p className="text-[11px] text-muted-foreground">
													{t("workspaceScope.delivery.stagedFiles", {
														count: preview.commit.stagedFileCount,
													})}
												</p>
											</div>
										) : null}
									</div>
								);
							})}
						</div>
					) : null}

					{deliveryRunning ? (
						<div className="flex items-center gap-2 rounded-lg bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
							<LoaderCircleIcon className="size-4 animate-spin" />
							{t("workspaceScope.delivery.running")}
						</div>
					) : null}
					{deliveryError ? (
						<div className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
							{deliveryError}
						</div>
					) : null}

					<DialogFooter>
						{deliveryResults || deliveryError ? (
							<DialogClose asChild>
								<Button
									type="button"
									variant="outline"
									disabled={deliveryPreparing || deliveryRunning}
								>
									{t("workspaceScope.delivery.close")}
								</Button>
							</DialogClose>
						) : (
							<>
								<DialogClose asChild>
									<Button
										type="button"
										variant="outline"
										disabled={deliveryPreparing || deliveryRunning}
									>
										{t("workspaceScope.delivery.cancel")}
									</Button>
								</DialogClose>
								<Button
									type="button"
									onClick={handleDeliver}
									disabled={
										deliveryPreparing ||
										deliveryRunning ||
										!deliveryPreviews ||
										deliveryReadyCount === 0 ||
										hasInvalidCommitReview
									}
								>
									{deliveryRunning ? (
										<LoaderCircleIcon className="animate-spin" />
									) : (
										<GitPullRequestArrowIcon />
									)}
									{t("workspaceScope.delivery.confirmReviewed", {
										count: deliveryReadyCount,
									})}
								</Button>
							</>
						)}
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</div>
	);
}
