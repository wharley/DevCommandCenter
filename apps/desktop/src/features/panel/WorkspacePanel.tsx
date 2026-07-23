import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import {
	WorkspaceEditorSurface,
	type DiffAnnotationRequest,
	type DiffAnnotationSubmit,
} from "@/features/editor/WorkspaceEditorSurface";
import { FileTabsSurface } from "@/features/editor/file-tabs-surface";
import { WorkspaceMissionSpecSurface } from "@/features/editor/WorkspaceMissionSpecSurface";
import { WorkspaceMergeConflictResolver } from "@/features/merge/WorkspaceMergeConflictResolver";
import type {
	AgentResolutionRunRequest,
	AgentResolutionRunResult,
} from "@/features/merge/agent-conflict-resolution";
import { DccWorkbenchChatHeader } from "@/features/sessions/dcc-workbench-chat-header";
import {
	DelegationDialog,
	type DelegationDialogPrefill,
	type ManualDelegationRequest,
} from "@/features/sessions/delegation-dialog";
import type { AgentInitiatedDelegationRequest } from "@/features/sessions/agent-delegation-request";
import { ActiveThreadViewport } from "./ActiveThreadViewport";
import { DiffReviewTray, type ReviewAnnotation } from "./diff-review-tray";
import { WorkspaceComposer } from "@/features/composer";
import { sessionThreadHistoryQueryOptions } from "@/features/sessions/session-thread-history";
import {
	composerTurnFromRaw,
	type ComposerSubmittedTurn,
} from "@/features/composer/composer-turn";
import type { AppUpdateInfo } from "@/features/updater";
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
import { workspaceRailDisplayTitle } from "@/features/workspaces/workspace-rail-shared";
import {
	buildMissionSpecFilename,
	getComposerEffortKey,
} from "@/features/composer/WorkspaceComposer.logic";
import { buildPlanDelegationPrompt } from "./plan-content";
import { loadEffortSelection } from "@/features/composer/draftStorage";
import {
	DEFAULT_EFFORT_LEVELS,
	resolveEffectiveEffort,
} from "@/features/composer/effort";
import {
	computeMissionSpecHash,
	parseMissionValidationPersistence,
} from "@/features/spec/mission-spec-content";
import type { WorkspaceSurfaceSelection } from "./workspace-surface";

/** Composer draft injection request; the nonce lets a repeated annotation re-fire. */
type ComposerPrefill = { text: string; nonce: number };

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
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
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
	onResumeSession: () => void;
	onAbortSession: () => void;
	onDelegate: (request: ManualDelegationRequest) => Promise<void>;
	onAgentDelegate: (request: AgentInitiatedDelegationRequest) => Promise<void>;
	sessionActionSessionId: string | null;
	updateInfo: AppUpdateInfo;
	isInstallingUpdate: boolean;
	onInstallUpdate: () => void;
	surfaceSelection: WorkspaceSurfaceSelection | null;
	onCloseSurface: () => void;
	onOpenPlanSidebar: () => void;
	onImplementPlanInNewThread: (input: {
		planMarkdown: string;
		planTitle: string | null;
	}) => void;
	terminalScopes?: TerminalScopeTarget[];
	onOpenTerminal?: (request: OpenTerminalRequest) => void;
	externalComposerPrefill?: ComposerPrefill | null;
	/** Current inspector visibility — picks the open vs. close affordance. */
	inspectorCollapsed?: boolean;
	/** Toggles the inspector open/closed — wired to the header control. */
	onToggleInspector?: () => void;
	/** Reveals the inspector to review the current Git changes. */
	onReviewChanges?: () => void;
	/** Opens the inspector and previews an implementation delegation diff. */
	onReviewDelegation?: (delegationId: string) => void;
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
	workspaceName,
	workspaceBranch,
	workspacePath,
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
	onResumeSession,
	onAbortSession,
	onDelegate,
	onAgentDelegate,
	sessionActionSessionId,
	updateInfo,
	isInstallingUpdate,
	onInstallUpdate,
	surfaceSelection,
	onCloseSurface,
	onOpenPlanSidebar,
	onImplementPlanInNewThread,
	terminalScopes,
	onOpenTerminal,
	externalComposerPrefill,
	inspectorCollapsed,
	onToggleInspector,
	onReviewChanges,
	onReviewDelegation,
	onResolveConflictWithAgent,
	onOpenAgentSession,
	onMergeConflictStateChanged,
	delegateSignal,
}: WorkspacePanelProps) {
	const [composerPrefill, setComposerPrefill] = useState<ComposerPrefill | null>(
		null,
	);
	const [reviewAnnotations, setReviewAnnotations] = useState<ReviewAnnotation[]>(
		[],
	);
	const [delegateOpen, setDelegateOpen] = useState(false);
	const [delegatePrefill, setDelegatePrefill] =
		useState<DelegationDialogPrefill | null>(null);
	const [delegateSubmitting, setDelegateSubmitting] = useState(false);
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
	const delegatePrefillNonceRef = useRef(0);
	const lastDelegateSignalRef = useRef(delegateSignal ?? 0);

	// WorkspacePanel stays mounted while the active workspace changes. Clear its
	// transient UI state so a plan delegation from the previous workspace cannot
	// be offered or submitted from the newly selected one.
	useEffect(() => {
		setComposerPrefill(null);
		setReviewAnnotations([]);
		setDelegateOpen(false);
		setDelegatePrefill(null);
		setDelegateSubmitting(false);
	}, [workspaceId]);

	useEffect(() => {
		const signal = delegateSignal ?? 0;
		if (signal > lastDelegateSignalRef.current) {
			lastDelegateSignalRef.current = signal;
			setDelegatePrefill(null);
			setDelegateOpen(true);
		}
	}, [delegateSignal]);

	useEffect(() => {
		if (externalComposerPrefill) {
			setComposerPrefill(externalComposerPrefill);
		}
	}, [externalComposerPrefill]);

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
			return composerTurnFromRaw(content, { effort });
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
				targetSessionId: surfaceSelection?.kind === "git-diff"
					? surfaceSelection.file.targetSessionId ?? null
					: null,
			});
		},
		[buildAnnotationTurn, onSubmitPrompt, surfaceSelection],
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
	const handleDelegateSubmit = useCallback(
		async (request: ManualDelegationRequest) => {
			setDelegateSubmitting(true);
			try {
				await onDelegate(request);
				setDelegatePrefill(null);
				setDelegateOpen(false);
			} finally {
				setDelegateSubmitting(false);
			}
		},
		[onDelegate],
	);
	const handleOpenManualDelegation = useCallback(() => {
		setDelegatePrefill(null);
		setDelegateOpen(true);
	}, []);

	const effectiveSessionId = selectedSessionId ?? sessions[0]?.session.id ?? null;
	const threadHistoryQuery = useQuery(
		sessionThreadHistoryQueryOptions(effectiveSessionId, {
			scope: sessionQueryScope,
			refetchInterval: false,
		}),
	);
	const selectedSessionTitle =
		sessions.find((session) => session.session.id === effectiveSessionId)?.thread
			.title ?? workspaceName;
	const pathCaption =
		workspacePath && workspacePath.length > 0
			? workspacePath.length > 52
				? `…${workspacePath.slice(-51)}`
				: workspacePath
			: null;
	const historyEvents = threadHistoryQuery.data ?? [];
	const hasLoaded = sessionSnapshot
		? Boolean(threadHistoryQuery.isFetched || sessionEvents.length > 0)
		: true;
	const hasEmptyThread = !sessionSnapshot;
	const sessionState = sessionSnapshot?.state ?? null;
	const lastTurnState = sessionSnapshot?.lastTurnState ?? null;
	const isGitRepo = Boolean(workspaceBranch) || Boolean(workspacePath);
	// Hoisted here (always mounted) so the header's changes pill has data even while
	// the inspector is collapsed. Shares React Query's cache with the inspector's own
	// useWorkspaceGitStatus call — one network request, not two.
	const gitStatusQuery = useWorkspaceGitStatus(workspacePath);
	const gitChangeSummary = useMemo(() => {
		const data = gitStatusQuery.data;
		if (!data) {
			return null;
		}
		const entries = [...data.staged, ...data.unstaged];
		const files = new Set(entries.map((entry) => entry.path)).size;
		if (files === 0) {
			return null;
		}
		return {
			files,
			additions: entries.reduce((sum, entry) => sum + entry.insertions, 0),
			deletions: entries.reduce((sum, entry) => sum + entry.deletions, 0),
		};
	}, [gitStatusQuery.data]);
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
	const planFollowUpState = useMemo(
		() => derivePlanFollowUpState(messages),
		[messages],
	);
	const activePlanMessage = planFollowUpState.activePlanMessage;
	const latestPlanMessage = planFollowUpState.latestPlanMessage;
	const activePlanTitle =
		activePlanMessage?.plan?.title ?? (activePlanMessage ? "Plan" : null);
	const activePlanMarkdown =
		activePlanMessage?.plan?.markdown ?? activePlanMessage?.content ?? null;
	const showPlanFollowUpPrompt = planFollowUpState.showPlanFollowUpPrompt;
	const handleDelegatePlan = useCallback(() => {
		if (!activePlanMarkdown) {
			return;
		}
		delegatePrefillNonceRef.current += 1;
		setDelegatePrefill({
			nonce: delegatePrefillNonceRef.current,
			instruction: buildPlanDelegationPrompt(activePlanMarkdown),
			mode: "implement",
			contextPolicy: "full_reanchor",
			targetProviderId: "codex",
			targetModelId: "gpt-5.5",
		});
		setDelegateOpen(true);
	}, [activePlanMarkdown]);
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

	const surfaceContent = surfaceSelection ? (
		surfaceSelection.kind === "merge-conflict" ? (
			<WorkspaceMergeConflictResolver
				workspaceRoot={surfaceSelection.workspaceRoot}
				currentWorkspaceLabel={workspaceRailDisplayTitle({
					name: workspaceName,
					branch: workspaceBranch,
				})}
				baseBranch={surfaceSelection.baseBranch}
				forgeLogin={surfaceSelection.forgeLogin}
				onClose={onCloseSurface}
				onStateChanged={() =>
					onMergeConflictStateChanged(surfaceSelection.workspaceRoot)
				}
				onResolveWithAgent={onResolveConflictWithAgent}
				onOpenAgentSession={onOpenAgentSession}
			/>
		) : surfaceSelection.kind === "git-diff" ? (
			<WorkspaceEditorSurface
				workspaceRoot={workspacePath}
				selection={surfaceSelection.file}
				onClose={onCloseSurface}
				onSubmitAnnotation={handleSubmitAnnotation}
				onEditInComposer={handleEditAnnotationInComposer}
				onAddToReview={handleAddToReview}
			/>
		) : surfaceSelection.kind === "file-edit" ? (
			<FileTabsSurface
				workspaceRoot={workspacePath}
				path={surfaceSelection.path}
				name={surfaceSelection.name}
				openRequestId={surfaceSelection.requestId}
				focusLine={surfaceSelection.focusLine ?? null}
				onClose={onCloseSurface}
				onSubmitAnnotation={handleSubmitAnnotation}
				onEditInComposer={handleEditAnnotationInComposer}
				onAddToReview={handleAddToReview}
			/>
		) : (
			<WorkspaceMissionSpecSurface
				spec={surfaceSelection.spec}
				onClose={onCloseSurface}
			/>
		)
	) : (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			<header
				className={[
					"border-b border-border/60 px-4 py-3",
					"pl-[calc(env(safe-area-inset-left)+1rem)] pr-[calc(env(safe-area-inset-right)+1rem)]",
				].join(" ")}
			>
				<DccWorkbenchChatHeader
					threadTitle={selectedSessionTitle}
					projectBadgeLabel={workspaceBranch || null}
					isGitRepo={isGitRepo}
					pathCaption={pathCaption}
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
					onOpenDelegate={handleOpenManualDelegation}
					sessionActionSessionId={sessionActionSessionId}
					updateInfo={updateInfo}
					isInstallingUpdate={isInstallingUpdate}
					onInstallUpdate={onInstallUpdate}
					onOpenTerminal={onOpenTerminal ? openPreferredTerminal : undefined}
					terminalScopes={terminalScopes}
					workspacePath={workspacePath}
					gitChangeSummary={gitChangeSummary}
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
					selectedProviderLabel={selectedProviderLabel}
					selectedModelLabel={selectedModelLabel}
					sessionState={sessionState}
					lastTurnState={lastTurnState}
					pendingPrompt={pendingPrompt}
					workspacePath={workspacePath}
					workspaceId={workspaceId}
					providers={providerChoices}
					planMessageId={latestPlanMessage?.id ?? null}
					sessionId={effectiveSessionId}
					activeMissionSpecRelativePath={activeMissionSpecRelativePath}
					activeMissionSpecHash={activeMissionSpecHash}
					autoSaveMissionValidation={autoSaveMissionValidation}
					onStartSession={onStartSession}
					onSelectSession={onSelectSession}
					onSubmitPrompt={onSubmitPrompt}
					onReviewChanges={onReviewChanges}
					onReviewDelegation={onReviewDelegation}
					onDelegateTaskApprove={onAgentDelegate}
				/>

				<div className="border-t border-border/60 px-3 pb-3 pt-3 sm:px-4">
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
					workspacePath={workspacePath}
					workspaceBranch={workspaceBranch}
					showPlanFollowUpPrompt={showPlanFollowUpPrompt}
					planTitle={activePlanTitle}
					planMarkdown={activePlanMarkdown}
					onSelectProvider={onSelectProvider}
					onSelectModel={onSelectModel}
					onSubmitPrompt={onSubmitPrompt}
					onAbortSession={onAbortSession}
					onOpenPlanSidebar={onOpenPlanSidebar}
					onDelegatePlan={handleDelegatePlan}
					onImplementPlanInNewThread={onImplementPlanInNewThread}
				/>
				</div>
			</div>
		</div>
	);

	return (
		<>
			{surfaceContent}
			<DiffReviewTray
				annotations={reviewAnnotations}
				onRemove={handleRemoveReviewAnnotation}
				onClear={handleClearReview}
				onSubmit={handleSubmitReview}
			/>
			<DelegationDialog
				key={workspaceId}
				open={delegateOpen}
				providers={providerChoices}
				isSubmitting={delegateSubmitting}
				prefill={delegatePrefill}
				onOpenChange={setDelegateOpen}
				onSubmit={handleDelegateSubmit}
			/>
		</>
	);
}
