import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { FileDiff, LoaderCircle } from "lucide-react";
import type { WorkspaceSessionSummary, WorkspaceSetupReport } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	WorkspaceEditorSurface,
	type DiffAnnotationRequest,
	type DiffAnnotationSubmit,
} from "@/features/editor/WorkspaceEditorSurface";
import { FileTabsSurface } from "@/features/editor/file-tabs-surface";
import { resolveConfirmedFileSurfaceCloseHandler } from "@/features/editor/file-surface.logic";
import { WorkspaceMissionSpecSurface } from "@/features/editor/WorkspaceMissionSpecSurface";
import { WorkspacePlanSurface } from "@/features/editor/WorkspacePlanSurface";
import { WorkspaceMergeConflictResolver } from "@/features/merge/WorkspaceMergeConflictResolver";
import type {
	AgentResolutionRunRequest,
	AgentResolutionRunResult,
} from "@/features/merge/agent-conflict-resolution";
import { DccWorkbenchChatHeader } from "@/features/sessions/dcc-workbench-chat-header";
import type { ManualDelegationRequest } from "@/features/sessions/delegation-request";
import type { AgentInitiatedDelegationRequest } from "@/features/sessions/agent-delegation-request";
import { ActiveThreadViewport } from "./ActiveThreadViewport";
import { DiffReviewTray, type ReviewAnnotation } from "./diff-review-tray";
import { collectPendingPermissionRequests } from "./pending-permissions";
import { PendingPermissionPanel } from "./message-components";
import { WorkspaceComposer } from "@/features/composer";
import { sessionThreadHistoryQueryOptions } from "@/features/sessions/session-thread-history";
import { delegationTargetsFor } from "@/features/sessions/delegation-targets";
import {
	composerTurnFromRaw,
	type ComposerDelegationRequest,
	type ComposerSubmittedTurn,
} from "@/features/composer/composer-turn";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import type {
	OpenTerminalRequest,
	TerminalScopeTarget,
} from "@/features/terminal/terminal-scope";
import { projectWorkspaceMessages } from "./thread-projection";
import type {
	ProviderCatalog,
	CoreEvent,
	ProviderRuntimeConfig,
} from "@dcc/contracts";
import { derivePlanFollowUpState } from "./plan-follow-up";
import { useWorkspaceMissionSpecs } from "@/features/inspector/use-workspace-mission-specs";
import { useWorkspaceGitStatus } from "@/features/inspector/use-workspace-git-status";
import { useWorkspaceGitBranchDiff } from "@/features/inspector/use-workspace-git-branch-diff";
import { useWorkspacePrStatus } from "@/features/inspector/use-workspace-pr-status";
import { useWorkspaceForgeContext } from "@/features/inspector/use-workspace-forge-context";
import { resolveCommitMode } from "@/features/commit/WorkspaceCommitButton.logic";
import {
	useWorkspaceDelivery,
	prepareWorkspaceCommitMessage,
	type WorkspaceDeliveryCreateRequestInput,
} from "@/features/commit/use-workspace-delivery";
import {
	sanitizeWorkspaceCommitBody,
	sanitizeWorkspaceCommitSubject,
} from "@/features/commit/commit-message";
import { CreateChangeRequestDialog } from "@/features/commit/CreateChangeRequestDialog";
import { SyncBaseDialog } from "@/features/commit/SyncBaseDialog";
import { MergeConfirmDialog } from "@/features/commit/MergeConfirmDialog";
import type { ExecutionDockRunMode } from "@/features/composer/ExecutionDock.actions";
import { workspaceChangeRequestContext, workspaceGitStageAll } from "@/lib/workspace-api";
import { workspaceRailDisplayTitle } from "@/features/workspaces/workspace-rail-shared";
import {
	buildMissionSpecFilename,
	getComposerApprovalPolicyKey,
	getComposerEffortKey,
} from "@/features/composer/WorkspaceComposer.logic";
import {
	buildPlanDelegationPrompt,
	computePlanHash,
	parsePlanContent,
	planNeedsInput,
} from "./plan-content";
import {
	isPlanVersionApproved,
	isPlanVersionHandedOff,
} from "./plan-approval";
import { approvePlan, recordPlanHandoff } from "@/lib/session-api";
import {
	loadApprovalPolicy,
	loadEffortSelection,
} from "@/features/composer/draftStorage";
import {
	DEFAULT_EFFORT_LEVELS,
	resolveEffectiveEffort,
} from "@/features/composer/effort";
import {
	computeMissionSpecHash,
	parseMissionValidationPersistence,
} from "@/features/spec/mission-spec-content";
import type { WorkspaceSurfaceSelection } from "./workspace-surface";
import type { WorkspaceFileReference } from "@/components/workspace-file-reference";
import { buildSafeContinuationPrompt } from "./conversation-recovery";
import {
	canDockSecondarySurface,
	clampSecondarySurfaceWidthForContainer,
	MAX_SECONDARY_SURFACE_WIDTH,
	MIN_SECONDARY_SURFACE_WIDTH,
	persistRestorableSecondarySurfaceSelection,
	persistSecondarySurfaceWidth,
	readRestorableSecondarySurfaceSelection,
	readSecondarySurfaceWidth,
	resolveSecondarySurfaceRestoration,
} from "./secondary-surface-layout";
import { TurnReviewSurface } from "./turn-review-surface";
import { TurnReviewActionSummary } from "./turn-review-action-summary";
import {
	latestTurnReviewTerminalEvent,
	shouldInvalidateTurnReview,
} from "./turn-review.logic";
import { lastTurnReviewQueryKey } from "./turn-review-query";

/** Composer draft injection request; the nonce lets a repeated annotation re-fire. */
type ComposerPrefill = {
	text: string;
	nonce: number;
	mode?: "append" | "replace";
};

type ComposerPrefillConsumption = Pick<ComposerPrefill, "text" | "nonce">;

/** Formats a diff selection as a markdown context block for the agent prompt. */
function buildAnnotationContextBlock(request: DiffAnnotationRequest): string {
	const lineLabel =
		request.startLine === request.endLine
			? `linha ${request.startLine}`
			: `linhas ${request.startLine}–${request.endLine}`;
	// Tell the agent when the selection is deleted/old code so it doesn't try to
	// edit lines that no longer exist in the working tree.
	const sideNote =
		request.side === "original" ? ", código removido nesta mudança" : "";
	return [
		`Sobre \`${request.path}\` (${lineLabel}${sideNote}):`,
		"",
		"```",
		request.snippet,
		"```",
	].join("\n");
}

/** Combines the reviewer instruction (if any) with the diff context block. */
function buildAnnotationContent(
	request: DiffAnnotationRequest,
	instruction: string,
): string {
	const context = buildAnnotationContextBlock(request);
	const trimmed = instruction.trim();
	return trimmed.length > 0 ? `${trimmed}\n\n${context}` : context;
}

/** Composes every collected snippet (and its note) into one review prompt. */
function buildReviewContent(
	annotations: ReviewAnnotation[],
	overallInstruction: string,
): string {
	const parts: string[] = [];
	const overall = overallInstruction.trim();
	if (overall.length > 0) {
		parts.push(overall, "");
	}
	parts.push(`Revisão de ${annotations.length} trecho(s):`, "");
	annotations.forEach((annotation, index) => {
		const note = annotation.note.trim();
		parts.push(note.length > 0 ? `${index + 1}. ${note}` : `${index + 1}.`);
		parts.push(buildAnnotationContextBlock(annotation.request), "");
	});
	return parts.join("\n");
}

type WorkspacePanelProps = {
	workspaceId: string;
	/** Active bundle member whose per-turn evidence should be reviewed. */
	turnReviewWorkspaceId?: string | null;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
	workspaceSetupReport?: WorkspaceSetupReport | null;
	projectRootPath?: string | null;
	projectLabel?: string | null;
	projectIcon?: string | null;
	projectColor?: string | null;
	isIsolatedWorkspace?: boolean;
	workspaceContextProjects?: Array<{
		id: string;
		name: string;
		branch: string;
		icon?: string | null;
		color?: string | null;
	}>;
	sessionQueryScope?: string;
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
	/** Composer-initiated delegation; mode and context policy are derived upstream. */
	onDelegatePrompt: (request: ComposerDelegationRequest) => Promise<void>;
	onAgentDelegate: (request: AgentInitiatedDelegationRequest) => Promise<void>;
	sessionActionSessionId: string | null;
	surfaceSelection: WorkspaceSurfaceSelection | null;
	/** Workspace that owns `surfaceSelection`, used to avoid cross-workspace restore races. */
	surfaceSelectionWorkspaceId: string | null;
	/** Monotonic request from App to close/replace a dirty file surface safely. */
	fileSurfaceTransitionRequestId?: number;
	onFileSurfaceTransitionConfirmed?: () => void;
	/** The editor already confirmed its own close before invoking this callback. */
	onFileSurfaceClosed?: () => void;
	onCloseSurface: () => void;
	onOpenPlanSurface: () => void;
	onOpenTurnReview: (sessionId: string) => void;
	onOpenFileReference?: (reference: WorkspaceFileReference) => void;
	onImplementPlanInNewThread: (input: {
		planMarkdown: string;
		planTitle: string | null;
	}) => Promise<boolean>;
	terminalScopes?: TerminalScopeTarget[];
	onOpenTerminal?: (request: OpenTerminalRequest) => void;
	externalComposerPrefill?: ComposerPrefill | null;
	onExternalComposerPrefillConsumed?: (
		prefill: ComposerPrefillConsumption,
	) => void;
	composerFocusRequestKey?: number | null;
	/** Current inspector visibility — picks the open vs. close affordance. */
	inspectorCollapsed?: boolean;
	/** Toggles the inspector open/closed — wired to the header control. */
	onToggleInspector?: () => void;
	/** Reveals the inspector to review the current Git changes. */
	onReviewChanges?: () => void;
	onOpenMultiProjectDelivery?: () => void;
	onCompleteWorkspace?: (workspaceId: string) => Promise<void> | void;
	onCreateTaskFromBranch?: (branch: string) => Promise<void>;
	onRunRecommendedSetup?: (commands: string[]) => Promise<void>;
	onSkipRecommendedSetup?: () => Promise<void>;
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

export function WorkspacePanel({
	workspaceId,
	turnReviewWorkspaceId = workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	workspaceSetupReport = null,
	projectRootPath = null,
	projectLabel = null,
	projectIcon = null,
	projectColor = null,
	isIsolatedWorkspace = true,
	workspaceContextProjects = [],
	sessionQueryScope = "local",
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
	fileSurfaceTransitionRequestId = 0,
	onFileSurfaceTransitionConfirmed,
	onFileSurfaceClosed,
	onCloseSurface,
	onOpenPlanSurface,
	onOpenTurnReview,
	onOpenFileReference,
	onImplementPlanInNewThread,
	terminalScopes,
	onOpenTerminal,
	externalComposerPrefill,
	onExternalComposerPrefillConsumed,
	composerFocusRequestKey = null,
	inspectorCollapsed,
	onToggleInspector,
	onReviewChanges,
	onOpenMultiProjectDelivery,
	onCompleteWorkspace,
	onCreateTaskFromBranch,
	onRunRecommendedSetup,
	onSkipRecommendedSetup,
	onReviewDelegation,
	onRerunDelegation,
	onResolveConflictWithAgent,
	onOpenAgentSession,
	onMergeConflictStateChanged,
	delegateSignal,
}: WorkspacePanelProps) {
	const { t } = useTranslation("common");
	const workspaceSurfaceSelection =
		surfaceSelectionWorkspaceId === workspaceId ? surfaceSelection : null;
	const handleConfirmedFileSurfaceClose = useMemo(
		() =>
			resolveConfirmedFileSurfaceCloseHandler({
				onFileSurfaceClosed,
				onCloseSurface,
			}),
		[onFileSurfaceClosed, onCloseSurface],
	);
	const queryClient = useQueryClient();
	const activeTurnReviewSessionId =
		workspaceSurfaceSelection?.kind === "turn-review"
			? workspaceSurfaceSelection.sessionId
			: null;
	const terminalReviewEvent = useMemo(
		() =>
			activeTurnReviewSessionId
				? latestTurnReviewTerminalEvent(
						sessionEvents,
						activeTurnReviewSessionId,
					)
				: null,
		[activeTurnReviewSessionId, sessionEvents],
	);
	const reviewInvalidationRef = useRef<{
		identity: string | null;
		terminalEvent: string | null;
	}>({ identity: null, terminalEvent: null });
	useEffect(() => {
		const selection =
			workspaceSurfaceSelection?.kind === "turn-review"
				? workspaceSurfaceSelection
				: null;
		const identity = selection
			? `${selection.sessionId}:${selection.workspaceId}`
			: null;
		const previous = reviewInvalidationRef.current;
		const next = {
			identity,
			terminalEvent: terminalReviewEvent,
		};
		reviewInvalidationRef.current = next;
		if (!selection || !shouldInvalidateTurnReview(previous, next)) return;
		void queryClient.invalidateQueries({
			queryKey: lastTurnReviewQueryKey(
				selection.sessionId,
				selection.workspaceId,
			),
		});
	}, [
		queryClient,
		terminalReviewEvent,
		workspaceSurfaceSelection,
	]);
	const [composerPrefill, setComposerPrefill] = useState<ComposerPrefill | null>(
		null,
	);
	const [reviewAnnotations, setReviewAnnotations] = useState<ReviewAnnotation[]>(
		[],
	);
	const [isApprovingPlan, setIsApprovingPlan] = useState(false);
	const [secondarySurfaceWidth, setSecondarySurfaceWidth] = useState(() =>
		readSecondarySurfaceWidth(workspaceId),
	);
	const [secondarySurfaceContainerWidth, setSecondarySurfaceContainerWidth] =
		useState(0);
	const [isSecondarySurfaceResizing, setIsSecondarySurfaceResizing] =
		useState(false);
	const [secondarySurfaceContainer, setSecondarySurfaceContainer] =
		useState<HTMLDivElement | null>(null);
	const secondarySurfaceRef = useRef<HTMLElement | null>(null);
	const secondarySurfaceLastFocusRef = useRef<HTMLElement | null>(null);
	const wasSecondarySurfaceOpenRef = useRef(false);
	const secondarySurfaceResizeFrameRef = useRef<number | null>(null);
	const secondarySurfaceResizeRef = useRef<{
		startX: number;
		startWidth: number;
	} | null>(null);
	const secondarySurfaceWidthRef = useRef(secondarySurfaceWidth);
	secondarySurfaceWidthRef.current = secondarySurfaceWidth;
	const restoredSecondarySurfaceWorkspaceRef = useRef<string | null>(null);
	const setSecondarySurfaceContainerNode = useCallback(
		(node: HTMLDivElement | null) => {
			setSecondarySurfaceContainer(node);
		},
		[],
	);
	const openPreferredTerminal = useCallback(() => {
		if (!onOpenTerminal) return;
		const preferredScope =
			terminalScopes?.find((scope) => scope.kind === "worktree" && scope.cwd) ??
			terminalScopes?.find((scope) => Boolean(scope.cwd));
		if (preferredScope) {
			onOpenTerminal({ scope: preferredScope.kind });
		}
	}, [onOpenTerminal, terminalScopes]);
	const reviewIdRef = useRef(0);
	const planHandoffInFlightRef = useRef(false);

	// WorkspacePanel stays mounted while the active workspace changes. Clear its
	// transient UI state so a plan delegation from the previous workspace cannot
	// be offered or submitted from the newly selected one.
	useEffect(() => {
		setComposerPrefill(null);
		setReviewAnnotations([]);
		setIsApprovingPlan(false);
		planHandoffInFlightRef.current = false;
		setSecondarySurfaceWidth(readSecondarySurfaceWidth(workspaceId));
		restoredSecondarySurfaceWorkspaceRef.current = null;
	}, [workspaceId]);

	useEffect(() => {
		if (externalComposerPrefill) {
			setComposerPrefill(externalComposerPrefill);
			onExternalComposerPrefillConsumed?.(externalComposerPrefill);
		}
	}, [externalComposerPrefill, onExternalComposerPrefillConsumed]);

	const updateSecondarySurfaceWidth = useCallback(
		(nextWidth: number, persist = false) => {
			const width = clampSecondarySurfaceWidthForContainer(
				nextWidth,
				secondarySurfaceContainerWidth,
			);
			secondarySurfaceWidthRef.current = width;
			setSecondarySurfaceWidth(width);
			if (persist) persistSecondarySurfaceWidth(workspaceId, width);
		},
		[secondarySurfaceContainerWidth, workspaceId],
	);

	useEffect(() => {
		const container = secondarySurfaceContainer;
		if (!container) return;
		const updateContainerWidth = () => {
			setSecondarySurfaceContainerWidth(
				Math.round(container.getBoundingClientRect().width),
			);
		};
		updateContainerWidth();
		if (typeof ResizeObserver === "undefined") {
			window.addEventListener("resize", updateContainerWidth);
			return () => window.removeEventListener("resize", updateContainerWidth);
		}
		const observer = new ResizeObserver(updateContainerWidth);
		observer.observe(container);
		return () => observer.disconnect();
	}, [secondarySurfaceContainer]);

	useEffect(() => {
		if (!isSecondarySurfaceResizing) return;

		let pendingClientX: number | null = null;
		const flushResize = () => {
			secondarySurfaceResizeFrameRef.current = null;
			const resize = secondarySurfaceResizeRef.current;
			if (!resize || pendingClientX === null) return;
			const clientX = pendingClientX;
			pendingClientX = null;
			// The companion surface is on the right; dragging its left edge left widens it.
			updateSecondarySurfaceWidth(resize.startWidth + resize.startX - clientX);
		};
		const onMouseMove = (event: MouseEvent) => {
			pendingClientX = event.clientX;
			if (secondarySurfaceResizeFrameRef.current === null) {
				secondarySurfaceResizeFrameRef.current = requestAnimationFrame(flushResize);
			}
		};
		const onMouseUp = () => {
			if (secondarySurfaceResizeFrameRef.current !== null) {
				cancelAnimationFrame(secondarySurfaceResizeFrameRef.current);
				flushResize();
			}
			persistSecondarySurfaceWidth(
				workspaceId,
				secondarySurfaceWidthRef.current,
			);
			secondarySurfaceResizeRef.current = null;
			setIsSecondarySurfaceResizing(false);
		};

		const previousCursor = document.body.style.cursor;
		const previousUserSelect = document.body.style.userSelect;
		document.body.style.cursor = "col-resize";
		document.body.style.userSelect = "none";
		window.addEventListener("mousemove", onMouseMove);
		window.addEventListener("mouseup", onMouseUp);
		return () => {
			if (secondarySurfaceResizeFrameRef.current !== null) {
				cancelAnimationFrame(secondarySurfaceResizeFrameRef.current);
				secondarySurfaceResizeFrameRef.current = null;
			}
			document.body.style.cursor = previousCursor;
			document.body.style.userSelect = previousUserSelect;
			window.removeEventListener("mousemove", onMouseMove);
			window.removeEventListener("mouseup", onMouseUp);
		};
	}, [isSecondarySurfaceResizing, updateSecondarySurfaceWidth, workspaceId]);

	useEffect(() => {
		if (
			restoredSecondarySurfaceWorkspaceRef.current !== workspaceId &&
			surfaceSelection === null
		) {
			return;
		}
		if (
			surfaceSelection !== null &&
			surfaceSelectionWorkspaceId !== workspaceId
		) {
			return;
		}
		const selection = workspaceSurfaceSelection?.kind === "plan" ? "plan" : null;
		persistRestorableSecondarySurfaceSelection(workspaceId, selection);
	}, [
		surfaceSelection,
		surfaceSelectionWorkspaceId,
		workspaceId,
		workspaceSurfaceSelection?.kind,
	]);

	useEffect(() => {
		const decision = resolveSecondarySurfaceRestoration({
			workspaceId,
			restoredWorkspaceId: restoredSecondarySurfaceWorkspaceRef.current,
			surfaceWorkspaceId: surfaceSelectionWorkspaceId,
			hasSurfaceSelection: surfaceSelection !== null,
			storedSelection: readRestorableSecondarySurfaceSelection(workspaceId),
		});
		if (decision === "wait") return;
		restoredSecondarySurfaceWorkspaceRef.current = workspaceId;
		if (decision === "plan") {
			onOpenPlanSurface();
		}
	}, [
		onOpenPlanSurface,
		surfaceSelection,
		surfaceSelectionWorkspaceId,
		workspaceId,
	]);

	// Build a turn that honors the workspace's persisted effort/ultrathink so a
	// direct send matches what the user would get from the composer.
	const buildAnnotationTurn = useCallback(
		(content: string) => {
			const selectedProvider = providerChoices.find(
				(provider) => provider.id === selectedProviderId,
			);
			const selectedModel =
				selectedProvider?.models.find((model) => model.id === selectedModelId) ??
				null;
			const persisted = loadEffortSelection(getComposerEffortKey(workspaceId));
			const effort = resolveEffectiveEffort({
				selectedEffort: persisted.effort,
				supportedEfforts: selectedModel?.effortLevels ?? DEFAULT_EFFORT_LEVELS,
				ultrathinkSelected: persisted.ultrathink,
				rawPrompt: content,
			});
			const approvalPolicy = loadApprovalPolicy(
				getComposerApprovalPolicyKey(workspaceId, selectedProviderId),
				selectedProvider?.capabilities.approvalPolicies ?? [],
			);
			return composerTurnFromRaw(content, { effort, approvalPolicy });
		},
		[providerChoices, selectedModelId, selectedProviderId, workspaceId],
	);

	const handleSubmitAnnotation = useCallback(
		({ request, instruction, newSession }: DiffAnnotationSubmit) => {
			const turn = buildAnnotationTurn(
				buildAnnotationContent(request, instruction),
			);
			void onSubmitPrompt(turn, {
				forceNewSession: newSession,
				targetSessionId: workspaceSurfaceSelection?.kind === "git-diff"
					? workspaceSurfaceSelection.file.targetSessionId ?? null
					: null,
			});
		},
		[buildAnnotationTurn, onSubmitPrompt, workspaceSurfaceSelection],
	);
	const handleEditAnnotationInComposer = useCallback(
		({
			request,
			instruction,
		}: {
			request: DiffAnnotationRequest;
			instruction: string;
		}) => {
			setComposerPrefill((prev) => ({
				text: buildAnnotationContent(request, instruction),
				nonce: (prev?.nonce ?? 0) + 1,
			}));
		},
		[],
	);
	const handleAddToReview = useCallback(
		({ request, note }: { request: DiffAnnotationRequest; note: string }) => {
			reviewIdRef.current += 1;
			const id = `rev-${reviewIdRef.current}`;
			setReviewAnnotations((prev) => [...prev, { id, request, note }]);
		},
		[],
	);
	const handleRemoveReviewAnnotation = useCallback((id: string) => {
		setReviewAnnotations((prev) => prev.filter((item) => item.id !== id));
	}, []);
	const handleClearReview = useCallback(() => setReviewAnnotations([]), []);
	const handleSubmitReview = useCallback(
		({ instruction, newSession }: { instruction: string; newSession: boolean }) => {
			if (reviewAnnotations.length === 0) {
				return;
			}
			const turn = buildAnnotationTurn(
				buildReviewContent(reviewAnnotations, instruction),
			);
			void onSubmitPrompt(turn, { forceNewSession: newSession });
			setReviewAnnotations([]);
			onCloseSurface();
		},
		[buildAnnotationTurn, onCloseSurface, onSubmitPrompt, reviewAnnotations],
	);
	const selectedSessionBelongsToWorkspace = Boolean(
		selectedSessionId &&
			sessions.some(
				(summary) =>
					summary.session.id === selectedSessionId &&
					summary.session.workspaceId === workspaceId,
			),
	);
	const effectiveSessionId = selectedSessionBelongsToWorkspace
		? selectedSessionId
		: (sessions.find((summary) => summary.session.workspaceId === workspaceId)
				?.session.id ?? null);
	const threadHistoryQuery = useQuery(
		sessionThreadHistoryQueryOptions(effectiveSessionId, {
			scope: sessionQueryScope,
			refetchInterval: false,
		}),
	);
	const selectedSessionTitle =
		sessions.find((session) => session.session.id === effectiveSessionId)?.thread
			.title ?? workspaceName;
	const historyEvents = threadHistoryQuery.data ?? [];
	const hasLoaded = sessionSnapshot
		? Boolean(threadHistoryQuery.isFetched || sessionEvents.length > 0)
		: true;
	const hasEmptyThread = !sessionSnapshot;
	const sessionState = sessionSnapshot?.state ?? null;
	const lastTurnState = sessionSnapshot?.lastTurnState ?? null;
	// Hoisted here (always mounted) so the execution dock stays accurate even while
	// the inspector is collapsed. These queries share React Query's cache with the
	// Inspector, so opening the panel does not create duplicate requests.
	const gitStatusQuery = useWorkspaceGitStatus(workspacePath);
	const branchDiffQuery = useWorkspaceGitBranchDiff(workspacePath);
	const currentBranch = gitStatusQuery.data?.currentBranch ?? null;
	const forgeContextQuery = useWorkspaceForgeContext(workspacePath);
	const forgeContext = forgeContextQuery.data ?? null;
	const forgeLogin = forgeContext?.effectiveLogin ?? null;
	const forgeRequestLabel: "PR" | "MR" = forgeContext?.provider === "gitlab" ? "MR" : "PR";
	const pullRequestQuery = useWorkspacePrStatus(workspacePath, currentBranch, forgeLogin);
	const [syncBaseConfirmationOpen, setSyncBaseConfirmationOpen] = useState(false);
	const [mergeConfirmationOpen, setMergeConfirmationOpen] = useState(false);
	const [commitPreview, setCommitPreview] = useState<{
		mode: "commit" | "commit-and-push";
		stagedFileCount: number;
		stagedFingerprint: string;
		body: string | null;
		preparing: boolean;
	} | null>(null);
	const [commitPreviewMessage, setCommitPreviewMessage] = useState("");
	const [commitPreparationBusy, setCommitPreparationBusy] = useState(false);
	const commitPreparationBusyRef = useRef(false);
	const commitMode = resolveCommitMode({
		branch: currentBranch || workspaceBranch,
		prStatus: pullRequestQuery.data,
		gitStatus: gitStatusQuery.data,
	});
	const [createRequestDraft, setCreateRequestDraft] = useState<boolean | null>(null);
	const changeRequestContextQuery = useQuery({
		queryKey: ["workspaceChangeRequestContext", workspacePath?.trim() ?? "", forgeLogin ?? ""],
		queryFn: () => workspaceChangeRequestContext({
			workspaceRoot: workspacePath?.trim() ?? "",
			forgeLogin,
		}),
		enabled: Boolean(workspacePath?.trim() && createRequestDraft !== null),
		staleTime: 15_000,
	});
	const dialogRequestLabel: "PR" | "MR" =
		changeRequestContextQuery.data?.requestLabel === "MR" ? "MR" : forgeRequestLabel;
	const gitChangeSummary = useMemo(() => {
		const status = gitStatusQuery.data;
		const branchDiff = branchDiffQuery.data;
		if (!status && !branchDiff && !pullRequestQuery.data) {
			return null;
		}
		const localEntries = status ? [...status.staged, ...status.unstaged] : [];
		const branchEntries = branchDiff?.changes ?? [];
		const files = new Set(localEntries.map((entry) => entry.path)).size;
		const branchFiles = new Set(branchEntries.map((entry) => entry.path)).size;
		return {
			files,
			additions: localEntries.reduce((sum, entry) => sum + entry.insertions, 0),
			deletions: localEntries.reduce((sum, entry) => sum + entry.deletions, 0),
			branchFiles,
			branchAdditions: branchEntries.reduce((sum, entry) => sum + entry.insertions, 0),
			branchDeletions: branchEntries.reduce((sum, entry) => sum + entry.deletions, 0),
			aheadOfRemoteCount: status?.aheadOfRemoteCount ?? 0,
			pullRequestState: pullRequestQuery.data?.state ?? null,
			pullRequestNumber: pullRequestQuery.data?.number ?? null,
		};
	}, [branchDiffQuery.data, gitStatusQuery.data, pullRequestQuery.data]);
	const gitStatusState =
		gitStatusQuery.isLoading || branchDiffQuery.isLoading
			? "loading"
			: gitStatusQuery.isError || branchDiffQuery.isError
				? "error"
				: "ready";
	const hasLocalChanges = (gitChangeSummary?.files ?? 0) > 0;
	const delivery = useWorkspaceDelivery({
		workspaceRoot: workspacePath,
		forgeLogin,
		baseBranch: pullRequestQuery.data?.baseBranch ?? branchDiffQuery.data?.baseBranch ?? workspaceBranch,
		requestLabel: forgeRequestLabel,
		stagedCount: gitStatusQuery.data?.staged.length ?? 0,
		hasLocalChanges,
		multiProject: workspaceContextProjects.length > 1,
		onReview: onReviewChanges ?? onToggleInspector ?? (() => undefined),
		onRequestSyncBase: () => setSyncBaseConfirmationOpen(true),
		onRequestMerge: () => setMergeConfirmationOpen(true),
		onCompleteWorkspace: () => onCompleteWorkspace?.(workspaceId),
		onOpenMultiProject: onOpenMultiProjectDelivery,
		onCreateRequest: (draft) => setCreateRequestDraft(draft),
		queryClient,
		t,
	});
	const runDeliveryAction = useCallback(
		async (mode: ExecutionDockRunMode) => {
			if ((mode === "commit" || mode === "commit-and-push") && workspacePath?.trim()) {
				// This preparation can involve staging and a provider request. Lock it
				// synchronously so rapid clicks cannot enqueue duplicate previews before
				// React has rendered the busy state.
				if (commitPreparationBusyRef.current) return;
				commitPreparationBusyRef.current = true;
				setCommitPreparationBusy(true);
				setCommitPreviewMessage("");
				setCommitPreview({
					mode,
					stagedFileCount: 0,
					stagedFingerprint: "",
					body: null,
					preparing: true,
				});
				try {
					if ((gitStatusQuery.data?.staged.length ?? 0) === 0) {
						await workspaceGitStageAll({
							workspaceRoot: workspacePath.trim(),
							relativePath: ".",
						});
					}
					const suggestion = await prepareWorkspaceCommitMessage(workspacePath.trim(), {
						providerId: selectedProviderId,
						model: selectedModelId,
						providerRuntime: selectedProviderRuntime,
					});
					setCommitPreview({
						mode,
						stagedFileCount: suggestion.stagedFileCount ?? 0,
						stagedFingerprint: suggestion.stagedFingerprint ?? "",
						body: suggestion.body ?? null,
						preparing: false,
					});
					setCommitPreviewMessage(suggestion.subject);
				} catch (error) {
					setCommitPreview(null);
					toast.error(t("commit.preview.preparationFailed"), {
						description: error instanceof Error ? error.message : String(error),
					});
				} finally {
					commitPreparationBusyRef.current = false;
					setCommitPreparationBusy(false);
				}
				return;
			}
			await delivery.run(mode);
		},
		[
			delivery,
			gitStatusQuery.data?.staged.length,
			selectedModelId,
			selectedProviderId,
			selectedProviderRuntime,
			t,
			workspacePath,
		],
	);
	const messages = useMemo(
		() =>
			projectWorkspaceMessages(
				historyEvents,
				sessionEvents,
				effectiveSessionId,
				pendingPrompt,
			),
		[effectiveSessionId, historyEvents, pendingPrompt, sessionEvents],
	);
	const replaceComposerDraft = useCallback((text: string) => {
		if (!text.trim()) return;
		setComposerPrefill((previous) => ({
			text,
			nonce: (previous?.nonce ?? 0) + 1,
			mode: "replace",
		}));
	}, []);
	const handleContinueInterrupted = useCallback(
		async (originalPrompt: string | null) => {
			try {
				if (sessionState === "aborted") {
					await onResumeSession();
				}
				replaceComposerDraft(
					buildSafeContinuationPrompt({
						originalPrompt,
						preamble: t("conversation.message.continuePrompt"),
						originalLabel: t("conversation.message.originalPrompt"),
					}),
				);
			} catch (error) {
				toast.error(t("conversation.message.resumeFailed"), {
					description: error instanceof Error ? error.message : undefined,
				});
			}
		},
		[onResumeSession, replaceComposerDraft, sessionState, t],
	);
	const pendingPermissionRequests = useMemo(
		() => collectPendingPermissionRequests(messages),
		[messages],
	);
	const planFollowUpState = useMemo(
		() => derivePlanFollowUpState(messages),
		[messages],
	);
	const activePlanMessage = planFollowUpState.activePlanMessage;
	const latestPlanMessage = planFollowUpState.latestPlanMessage;
	const activePlanRawTitle = activePlanMessage?.plan?.title ?? null;
	const activePlanTitle = activePlanMessage
		? activePlanRawTitle === "Plan" || activePlanRawTitle === null
			? t("planSurface.label")
			: activePlanRawTitle
		: null;
	const activePlanNeedsInput = activePlanMessage
		? planNeedsInput(activePlanMessage.plan?.rawMarkdown ?? activePlanMessage.content)
		: false;
	const latestPlanMarkdown =
		latestPlanMessage?.plan?.markdown ?? latestPlanMessage?.content ?? null;
	const latestPlanRawTitle = latestPlanMessage?.plan?.title ?? null;
	const latestPlanTitle = latestPlanMessage
		? latestPlanRawTitle === "Plan" || latestPlanRawTitle === null
			? t("planSurface.label")
			: latestPlanRawTitle
		: null;
	const latestPlan =
		latestPlanMessage?.plan ??
		(latestPlanMessage ? parsePlanContent(latestPlanMessage.content) : null);
	const latestPlanVersion = messages.filter(
		(message) => message.role === "assistant" && message.plan?.isPlanLike,
	).length;
	const latestPlanHash = latestPlanMarkdown
		? computePlanHash(latestPlanMarkdown)
		: null;
	const isLatestPlanApproved = isPlanVersionApproved(
		{
			sessionId: effectiveSessionId,
			planMessageId: latestPlanMessage?.id ?? null,
			planVersion: latestPlanVersion,
			planHash: latestPlanHash,
		},
		historyEvents,
		sessionEvents,
	);
	const isLatestPlanHandedOff = isPlanVersionHandedOff(
		{
			sessionId: effectiveSessionId,
			planMessageId: latestPlanMessage?.id ?? null,
			planVersion: latestPlanVersion,
			planHash: latestPlanHash,
		},
		historyEvents,
		sessionEvents,
	);
	const latestPlanNeedsInput = latestPlanMessage
		? planNeedsInput(latestPlanMessage.plan?.rawMarkdown ?? latestPlanMessage.content)
		: false;
	const showPlanFollowUpPrompt = planFollowUpState.showPlanFollowUpPrompt;
	const isLatestPlanCurrent =
		latestPlanMessage !== null &&
		activePlanMessage?.id === latestPlanMessage.id;
	const isLatestPlanReadOnly =
		Boolean(latestPlanMessage) &&
		(!isLatestPlanCurrent || isLatestPlanHandedOff);
	const canExecuteLatestPlan =
		isLatestPlanApproved &&
		!latestPlanNeedsInput &&
		!isLatestPlanReadOnly;
	const handleApprovePlan = useCallback(async () => {
		if (
			!effectiveSessionId ||
			!latestPlanMessage ||
			!latestPlanHash ||
			latestPlanVersion <= 0 ||
			isApprovingPlan
		) {
			return;
		}
		setIsApprovingPlan(true);
		try {
			await approvePlan({
				sessionId: effectiveSessionId,
				planMessageId: latestPlanMessage.id,
				planVersion: latestPlanVersion,
				planHash: latestPlanHash,
			});
			await threadHistoryQuery.refetch();
			toast.success(t("planSurface.approvalSaved"));
		} catch (error) {
			const refreshed = await threadHistoryQuery.refetch();
			const approvalWasPersisted = isPlanVersionApproved(
				{
					sessionId: effectiveSessionId,
					planMessageId: latestPlanMessage.id,
					planVersion: latestPlanVersion,
					planHash: latestPlanHash,
				},
				refreshed.data ?? [],
				sessionEvents,
			);
			if (approvalWasPersisted) {
				toast.success(t("planSurface.approvalSaved"));
			} else {
				toast.error(t("planSurface.approvalFailed"), {
					description: error instanceof Error ? error.message : String(error),
				});
			}
		} finally {
			setIsApprovingPlan(false);
		}
	}, [
		effectiveSessionId,
		isApprovingPlan,
		latestPlanHash,
		latestPlanMessage,
		latestPlanVersion,
		sessionEvents,
		t,
		threadHistoryQuery,
	]);
	const handleRequestPlanRevision = useCallback(
		(prompt: string) => {
			const turn = buildAnnotationTurn(prompt);
			void onSubmitPrompt({
				...turn,
				envelope: {
					...turn.envelope,
					planMode: true,
					fastMode: false,
				},
			});
		},
		[buildAnnotationTurn, onSubmitPrompt],
	);
	const recordCurrentPlanHandoff = useCallback(
		async (action: "delegation" | "new_thread") => {
			if (
				!effectiveSessionId ||
				!latestPlanMessage ||
				!latestPlanHash ||
				latestPlanVersion <= 0
			) {
				return false;
			}
			try {
				await recordPlanHandoff({
					sessionId: effectiveSessionId,
					planMessageId: latestPlanMessage.id,
					planVersion: latestPlanVersion,
					planHash: latestPlanHash,
					action,
					targetSessionId: null,
				});
				await threadHistoryQuery.refetch();
				return true;
			} catch (error) {
				const refreshed = await threadHistoryQuery.refetch();
				const wasPersisted = isPlanVersionHandedOff(
					{
						sessionId: effectiveSessionId,
						planMessageId: latestPlanMessage.id,
						planVersion: latestPlanVersion,
						planHash: latestPlanHash,
					},
					refreshed.data ?? [],
					sessionEvents,
				);
				if (!wasPersisted) {
					toast.error(t("planSurface.handoffRecordFailed"), {
						description:
							error instanceof Error ? error.message : String(error),
					});
				}
				return wasPersisted;
			}
		},
		[
			effectiveSessionId,
			latestPlanHash,
			latestPlanMessage,
			latestPlanVersion,
			sessionEvents,
			t,
			threadHistoryQuery,
		],
	);
	// An approved plan already carries every answer a delegation needs, and plan
	// approval is itself the human checkpoint — so the handoff runs directly.
	const planImplementationTargets = useMemo(() => {
		return delegationTargetsFor(providerChoices, {
			allowFileEdits: true,
		});
	}, [providerChoices]);
	const handleDelegatePlan = useCallback(async ({
		providerId,
		modelId,
	}: {
		providerId: string;
		modelId: string | null;
	}) => {
		if (!latestPlanMarkdown || !canExecuteLatestPlan) {
			return;
		}
		const planImplementationTarget = planImplementationTargets.find(
			(target) => target.id === providerId,
		);
		if (!planImplementationTarget) {
			toast.error(t("planSurface.delegateNoTarget"));
			return;
		}
		if (planHandoffInFlightRef.current) {
			return;
		}
		const targetModelId = planImplementationTarget.models.some(
			(model) => model.id === modelId,
		)
			? modelId
			: (planImplementationTarget.models.find((model) => model.recommended)?.id ??
				planImplementationTarget.models[0]?.id ??
				null);
		planHandoffInFlightRef.current = true;
		try {
			await onDelegate({
				targetProviderId: planImplementationTarget.id,
				targetProviderIds: [planImplementationTarget.id],
				targetModelId,
				mode: "implement",
				contextPolicy: { type: "full_reanchor" },
				instruction: buildPlanDelegationPrompt(latestPlanMarkdown),
			});
			onCloseSurface();
			await recordCurrentPlanHandoff("delegation");
		} finally {
			planHandoffInFlightRef.current = false;
		}
	}, [
		canExecuteLatestPlan,
		latestPlanMarkdown,
		onCloseSurface,
		onDelegate,
		planImplementationTargets,
		recordCurrentPlanHandoff,
		t,
	]);
	const handleImplementPlanInNewThread = useCallback(async () => {
		if (
			!canExecuteLatestPlan ||
			!latestPlanMarkdown ||
			planHandoffInFlightRef.current
		) {
			return;
		}
		planHandoffInFlightRef.current = true;
		try {
			const started = await onImplementPlanInNewThread({
				planMarkdown: latestPlanMarkdown,
				planTitle: latestPlanTitle,
			});
			if (!started) {
				return;
			}
			onCloseSurface();
			await recordCurrentPlanHandoff("new_thread");
		} finally {
			planHandoffInFlightRef.current = false;
		}
	}, [
		canExecuteLatestPlan,
		latestPlanMarkdown,
		latestPlanTitle,
		onCloseSurface,
		onImplementPlanInNewThread,
		recordCurrentPlanHandoff,
	]);
	const missionSpecsQuery = useWorkspaceMissionSpecs(workspacePath);
	const missionSpecs = missionSpecsQuery.data?.specs ?? [];
	const preferredSpecName = buildMissionSpecFilename(workspaceBranch);
	const activeMissionSpec =
		missionSpecs.find((spec) => spec.name === preferredSpecName) ??
		missionSpecs[0] ??
		null;
	const activeMissionSpecRelativePath = activeMissionSpec?.relativePath ?? null;
	const activeMissionSpecHash = activeMissionSpec
		? computeMissionSpecHash(activeMissionSpec.content)
		: null;
	const autoSaveMissionValidation =
		activeMissionSpec != null &&
		parseMissionValidationPersistence(activeMissionSpec.content) === "auto";

	const primaryContent = (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			<header
				className={[
					"border-b border-border/60 px-4 py-3",
					"pl-[calc(env(safe-area-inset-left)+1rem)] pr-[calc(env(safe-area-inset-right)+1rem)]",
				].join(" ")}
			>
				<DccWorkbenchChatHeader
					threadTitle={selectedSessionTitle}
					projectLabel={projectLabel}
					sessions={sessions}
					selectedSessionId={selectedSessionId}
					isLoadingSessions={isLoadingSessions}
					sessionSnapshot={sessionSnapshot}
					onSelectSession={onSelectSession}
					onStartSession={onStartSession}
					onCloseSession={onCloseSession}
					onRestoreSession={onRestoreSession}
					onOpenSessionSearch={onOpenSessionSearch}
					onResumeSession={onResumeSession}
					sessionActionSessionId={sessionActionSessionId}
					onOpenTerminal={onOpenTerminal ? openPreferredTerminal : undefined}
					terminalScopes={terminalScopes}
					workspacePath={workspacePath}
					inspectorCollapsed={inspectorCollapsed}
					onToggleInspector={onToggleInspector}
				/>
			</header>

			<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
				<ActiveThreadViewport
					messages={messages}
					hasLoaded={hasLoaded}
					isEmpty={hasEmptyThread}
					workspaceName={workspaceName}
					sessionState={sessionState}
					lastTurnState={lastTurnState}
					pendingPrompt={pendingPrompt}
					workspacePath={workspacePath}
					workspaceId={workspaceId}
					providers={providerChoices}
					providerId={sessionSnapshot?.providerId ?? selectedProviderId}
					planMessageId={latestPlanMessage?.id ?? null}
					planApproved={isLatestPlanApproved}
					planReadOnly={isLatestPlanReadOnly}
					sessionId={effectiveSessionId}
					activeMissionSpecRelativePath={activeMissionSpecRelativePath}
					activeMissionSpecHash={activeMissionSpecHash}
					autoSaveMissionValidation={autoSaveMissionValidation}
					onSelectSession={onSelectSession}
					onReviewChanges={onReviewChanges}
					onReviewDelegation={onReviewDelegation}
					onRerunDelegation={onRerunDelegation}
					onDelegateTaskApprove={onAgentDelegate}
					onEditPrompt={replaceComposerDraft}
					onContinueInterrupted={handleContinueInterrupted}
					onOpenPlan={onOpenPlanSurface}
					onOpenFileReference={onOpenFileReference}
				/>

				{effectiveSessionId ? (
					<PendingPermissionPanel
						sessionId={effectiveSessionId}
						requests={pendingPermissionRequests}
						onDelegateTaskApprove={onAgentDelegate}
					/>
				) : null}

				<div className="border-t border-border/60 px-3 pb-3 pt-3 sm:px-4">
					{effectiveSessionId && sessionSnapshot?.activeTurnId === null && lastTurnState ? (
						<Button
							type="button"
							variant="ghost"
							size="xs"
							className="mb-2 h-7 w-full max-w-xl justify-start gap-1.5 px-2 text-[11px] text-muted-foreground"
							onClick={() => onOpenTurnReview(effectiveSessionId)}
						>
							<FileDiff className="size-3.5" />
							<span>{t("turnReview.action")}</span>
							{turnReviewWorkspaceId ? (
								<TurnReviewActionSummary
									sessionId={effectiveSessionId}
									workspaceId={turnReviewWorkspaceId}
								/>
							) : null}
						</Button>
					) : null}
					<WorkspaceComposer
						draftKey={workspaceId}
						disabled={false}
						providerChoices={providerChoices}
						selectedProviderId={selectedProviderId}
						selectedModelId={selectedModelId}
						selectedProviderRuntime={selectedProviderRuntime}
						sessionSnapshot={sessionSnapshot}
						pendingPrompt={pendingPrompt}
						prefill={composerPrefill}
						focusRequestKey={composerFocusRequestKey}
						workspacePath={workspacePath}
						workspaceSetupReport={workspaceSetupReport}
						projectRootPath={projectRootPath}
						workspaceBranch={workspaceBranch}
						projectLabel={projectLabel}
						projectIcon={projectIcon}
						projectColor={projectColor}
						currentBranch={currentBranch}
						isIsolatedWorkspace={isIsolatedWorkspace}
						gitChangeSummary={gitChangeSummary}
						gitStatusState={gitStatusState}
						contextProjects={workspaceContextProjects}
						showPlanFollowUpPrompt={showPlanFollowUpPrompt}
						planTitle={activePlanTitle}
						planNeedsInput={activePlanNeedsInput}
						planApproved={isLatestPlanApproved}
						onSelectProvider={onSelectProvider}
						onSelectModel={onSelectModel}
						onSubmitPrompt={onSubmitPrompt}
						onSteerPrompt={onSteerPrompt}
						onQueuePrompt={onQueuePrompt}
						onDelegatePrompt={sessionSnapshot ? onDelegatePrompt : undefined}
						openDelegateMenuSignal={delegateSignal ?? 0}
						onAbortSession={onAbortSession}
						onReviewPlan={onOpenPlanSurface}
						onReviewChanges={onReviewChanges ?? onToggleInspector}
						commitMode={commitMode}
						forgeRequestLabel={forgeRequestLabel}
						deliveryBusy={delivery.busy || commitPreparationBusy}
						onRunDeliveryAction={runDeliveryAction}
						onCreateChangeRequest={(draft) => setCreateRequestDraft(draft)}
						onOpenMultiProjectDelivery={onOpenMultiProjectDelivery}
						onCreateTaskFromBranch={onCreateTaskFromBranch}
						onRunRecommendedSetup={onRunRecommendedSetup}
						onSkipRecommendedSetup={onSkipRecommendedSetup}
					/>
				</div>
			</div>
		</div>
	);
	const secondarySurfaceContent =
		workspaceSurfaceSelection?.kind === "turn-review" ? (
			<TurnReviewSurface
				sessionId={workspaceSurfaceSelection.sessionId}
				workspaceId={workspaceSurfaceSelection.workspaceId}
				onClose={onCloseSurface}
			/>
		) : workspaceSurfaceSelection?.kind === "git-diff" ? (
			<WorkspaceEditorSurface
				workspaceRoot={workspacePath}
				selection={workspaceSurfaceSelection.file}
				onClose={onCloseSurface}
				onSubmitAnnotation={handleSubmitAnnotation}
				onEditInComposer={handleEditAnnotationInComposer}
				onAddToReview={handleAddToReview}
			/>
		) : workspaceSurfaceSelection?.kind === "file-edit" ? (
			<FileTabsSurface
				workspaceRoot={workspacePath}
				path={workspaceSurfaceSelection.path}
				name={workspaceSurfaceSelection.name}
				openRequestId={workspaceSurfaceSelection.requestId}
				focusLine={workspaceSurfaceSelection.focusLine ?? null}
				focusColumn={workspaceSurfaceSelection.focusColumn ?? null}
				closeRequestId={fileSurfaceTransitionRequestId}
				onExternalCloseConfirmed={onFileSurfaceTransitionConfirmed}
				onClose={handleConfirmedFileSurfaceClose}
				onSubmitAnnotation={handleSubmitAnnotation}
				onEditInComposer={handleEditAnnotationInComposer}
				onAddToReview={handleAddToReview}
			/>
		) : workspaceSurfaceSelection?.kind === "plan" ? (
			latestPlan && latestPlanMarkdown ? (
				<WorkspacePlanSurface
					plan={latestPlan}
					version={latestPlanVersion}
					workspacePath={workspacePath}
					approved={isLatestPlanApproved}
					approving={isApprovingPlan}
					readOnly={isLatestPlanReadOnly}
					needsInput={latestPlanNeedsInput}
					onApprove={handleApprovePlan}
					onClose={onCloseSurface}
					onRequestRevision={handleRequestPlanRevision}
					delegationTargets={planImplementationTargets}
					onDelegate={handleDelegatePlan}
					onImplementInNewThread={handleImplementPlanInNewThread}
				/>
			) : (
				<div className="flex h-full items-center justify-center bg-background p-8 text-sm text-muted-foreground">
					The plan is no longer available in this session.
				</div>
			)
		) : null;
	const isSecondarySurfaceOpen = secondarySurfaceContent !== null;
	const shouldDockSecondarySurface =
		inspectorCollapsed !== false &&
		canDockSecondarySurface(secondarySurfaceContainerWidth);
	const visibleSecondarySurfaceWidth = clampSecondarySurfaceWidthForContainer(
		secondarySurfaceWidth,
		secondarySurfaceContainerWidth,
	);
	const requestSecondarySurfaceClose = useCallback(() => {
		onCloseSurface();
	}, [onCloseSurface]);

	useEffect(() => {
		if (isSecondarySurfaceOpen && !wasSecondarySurfaceOpenRef.current) {
			const activeElement = document.activeElement;
			secondarySurfaceLastFocusRef.current =
				activeElement instanceof HTMLElement ? activeElement : null;
			const frame = requestAnimationFrame(() => secondarySurfaceRef.current?.focus());
			wasSecondarySurfaceOpenRef.current = true;
			return () => cancelAnimationFrame(frame);
		}
		if (!isSecondarySurfaceOpen && wasSecondarySurfaceOpenRef.current) {
			wasSecondarySurfaceOpenRef.current = false;
			secondarySurfaceLastFocusRef.current?.focus();
			secondarySurfaceLastFocusRef.current = null;
		}
	}, [isSecondarySurfaceOpen]);

	// A pinned inspector already consumes the right side of the workbench. Keep the
	// conversation usable by presenting the companion surface as an overlay until
	// that inspector is collapsed again.
	const surfaceContent =
		workspaceSurfaceSelection?.kind === "merge-conflict" ? (
			<WorkspaceMergeConflictResolver
				workspaceRoot={workspaceSurfaceSelection.workspaceRoot}
				currentWorkspaceLabel={workspaceRailDisplayTitle({
					name: workspaceName,
					branch: workspaceBranch,
				})}
				baseBranch={workspaceSurfaceSelection.baseBranch}
				forgeLogin={workspaceSurfaceSelection.forgeLogin}
				onClose={onCloseSurface}
				onStateChanged={() =>
					onMergeConflictStateChanged(workspaceSurfaceSelection.workspaceRoot)
				}
				onResolveWithAgent={onResolveConflictWithAgent}
				onOpenAgentSession={onOpenAgentSession}
			/>
		) : workspaceSurfaceSelection?.kind === "mission-spec" ? (
			<WorkspaceMissionSpecSurface
				spec={workspaceSurfaceSelection.spec}
				onClose={onCloseSurface}
			/>
		) : (
			<div
				ref={setSecondarySurfaceContainerNode}
				className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden"
			>
				{primaryContent}
				{secondarySurfaceContent ? (
					<>
						<button
							type="button"
							aria-label={t("workbench.secondarySurface.close")}
							className={`absolute inset-0 z-30 cursor-default bg-black/20 backdrop-blur-[1px] ${
								shouldDockSecondarySurface ? "min-[1180px]:hidden" : ""
							}`}
							onClick={requestSecondarySurfaceClose}
						/>
						<div
							role="separator"
							aria-orientation="vertical"
							aria-label={t("workbench.secondarySurface.resize")}
							aria-valuemin={MIN_SECONDARY_SURFACE_WIDTH}
							aria-valuemax={MAX_SECONDARY_SURFACE_WIDTH}
							aria-valuenow={visibleSecondarySurfaceWidth}
							tabIndex={0}
							className={`relative z-40 hidden w-1.5 shrink-0 cursor-col-resize border-l border-border/60 bg-muted/20 hover:bg-muted/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
								shouldDockSecondarySurface ? "min-[1180px]:block" : ""
							}`}
							onMouseDown={(event) => {
								event.preventDefault();
								secondarySurfaceResizeRef.current = {
									startX: event.clientX,
									startWidth: visibleSecondarySurfaceWidth,
								};
								setIsSecondarySurfaceResizing(true);
							}}
							onKeyDown={(event) => {
								const keyboardWidths: Record<string, number> = {
									ArrowLeft: visibleSecondarySurfaceWidth + 16,
									ArrowRight: visibleSecondarySurfaceWidth - 16,
									Home: MIN_SECONDARY_SURFACE_WIDTH,
									End: MAX_SECONDARY_SURFACE_WIDTH,
								};
								const nextWidth = keyboardWidths[event.key];
								if (nextWidth === undefined) return;
								event.preventDefault();
								updateSecondarySurfaceWidth(nextWidth, true);
							}}
						/>
						<aside
							ref={secondarySurfaceRef}
							role="region"
							aria-label={t("workbench.secondarySurface.ariaLabel")}
							tabIndex={-1}
							className={`absolute inset-y-0 right-0 z-40 flex min-h-0 max-w-full flex-col overflow-hidden border-l border-border/60 bg-background shadow-2xl ${
								shouldDockSecondarySurface
									? "min-[1180px]:relative min-[1180px]:z-auto min-[1180px]:shrink-0 min-[1180px]:shadow-none"
									: ""
							}`}
							style={{ width: visibleSecondarySurfaceWidth }}
						>
							{secondarySurfaceContent}
						</aside>
					</>
				) : null}
			</div>
		);

	return (
		<>
			{surfaceContent}
			<Dialog
				open={commitPreview !== null}
				onOpenChange={(open) => {
					if (!open && !delivery.busy && !commitPreparationBusy) setCommitPreview(null);
				}}
			>
				<DialogContent className="sm:max-w-lg">
					<DialogHeader>
						<DialogTitle>{t("commit.preview.title")}</DialogTitle>
						<DialogDescription>{t("commit.preview.description")}</DialogDescription>
					</DialogHeader>
					{commitPreview?.preparing ? (
						<div
							className="flex min-h-52 items-center justify-center gap-3 rounded-lg border border-border/60 bg-muted/20 px-6 py-8 text-center"
							role="status"
							aria-live="polite"
						>
							<LoaderCircle className="size-5 shrink-0 animate-spin text-muted-foreground" />
							<div className="text-left">
								<p className="text-[13px] font-medium text-foreground">
									{t("commit.preview.preparing")}
								</p>
								<p className="mt-1 text-[11px] leading-4 text-muted-foreground">
									{t("commit.preview.preparingDescription")}
								</p>
							</div>
						</div>
					) : (
						<div className="grid gap-2 py-3">
							<label
								className="text-[12px] font-medium"
								htmlFor="workspace-commit-message-preview"
							>
								{t("commit.preview.label")}
							</label>
							<Textarea
								id="workspace-commit-message-preview"
								value={commitPreviewMessage}
								onChange={(event) => setCommitPreviewMessage(event.target.value)}
								className="min-h-24 resize-y font-mono text-[12px]"
								autoFocus
							/>
							<label className="text-[12px] font-medium" htmlFor="workspace-commit-body-preview">
								{t("commit.preview.bodyLabel")}
							</label>
							<Textarea
								id="workspace-commit-body-preview"
								value={commitPreview?.body ?? ""}
								onChange={(event) =>
									setCommitPreview((previous) =>
										previous ? { ...previous, body: event.target.value } : previous,
									)
								}
								className="min-h-20 resize-y font-mono text-[12px]"
							/>
							{commitPreview ? (
								<p className="text-[11px] text-muted-foreground">
									{t("commit.preview.source", { count: commitPreview.stagedFileCount })}
								</p>
							) : null}
						</div>
					)}
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							disabled={delivery.busy || commitPreparationBusy}
							onClick={() => setCommitPreview(null)}
						>
							{t("commit.preview.cancel")}
						</Button>
						<Button
							type="button"
							disabled={
								delivery.busy ||
								commitPreparationBusy ||
								!commitPreviewMessage.trim() ||
								!commitPreview ||
								commitPreview.preparing
							}
							onClick={async () => {
								if (!commitPreview) return;
								const pending = commitPreview;
								setCommitPreview(null);
								await delivery.run(
									pending.mode,
									sanitizeWorkspaceCommitSubject(commitPreviewMessage),
									sanitizeWorkspaceCommitBody(pending.body),
									pending.stagedFingerprint,
								);
							}}
						>
							{commitPreview?.mode === "commit"
								? t("composer.executionDock.actions.commit")
								: t("commit.preview.confirm")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
			<CreateChangeRequestDialog
				open={createRequestDraft !== null}
				onOpenChange={(open) => {
					if (!open && !delivery.busy) setCreateRequestDraft(null);
				}}
				requestLabel={dialogRequestLabel}
				headBranch={changeRequestContextQuery.data?.headBranch ?? currentBranch}
				baseBranch={changeRequestContextQuery.data?.baseBranch ?? branchDiffQuery.data?.baseBranch ?? workspaceBranch}
				defaultTitle={changeRequestContextQuery.data?.title ?? workspaceName}
				localFiles={gitChangeSummary?.files ?? 0}
				localAdditions={gitChangeSummary?.additions ?? 0}
				localDeletions={gitChangeSummary?.deletions ?? 0}
				loading={delivery.busy}
				initialDraft={createRequestDraft === true}
				onSubmit={async (input) => {
					const request: WorkspaceDeliveryCreateRequestInput = {
						workspaceRoot: workspacePath?.trim() ?? "",
						forgeLogin,
						title: input.title,
						body: input.body || null,
						draft: input.draft,
						includeLocalChanges: input.includeLocalChanges,
					};
					await delivery.createRequest(request);
					setCreateRequestDraft(null);
				}}
			/>
			<SyncBaseDialog
				open={syncBaseConfirmationOpen}
				onOpenChange={setSyncBaseConfirmationOpen}
				baseBranch={pullRequestQuery.data?.baseBranch ?? branchDiffQuery.data?.baseBranch ?? workspaceBranch}
				loading={delivery.busy}
				onConfirm={() => {
					setSyncBaseConfirmationOpen(false);
					void delivery.syncBase();
				}}
			/>
			<MergeConfirmDialog
				open={mergeConfirmationOpen}
				onOpenChange={setMergeConfirmationOpen}
				requestLabel={forgeRequestLabel}
				loading={delivery.busy}
				onConfirm={() => {
					setMergeConfirmationOpen(false);
					void delivery.merge();
				}}
			/>
			<DiffReviewTray
				annotations={reviewAnnotations}
				onRemove={handleRemoveReviewAnnotation}
				onClear={handleClearReview}
				onSubmit={handleSubmitReview}
			/>
		</>
	);
}
