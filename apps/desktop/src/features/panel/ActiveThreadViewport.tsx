import { DecisionEvaluationDetails } from "./DecisionEvaluationDetails";
import type { EditFromHereRequest } from "@/features/sessions/use-edit-from-here";
import { TurnReviewTimelineCard } from "./turn-review-timeline-card";
import {
	memo,
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { ChevronDown } from "lucide-react";
import { useStickToBottom } from "use-stick-to-bottom";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
	ConversationExecutionState,
	ConversationStartingIndicator,
	SessionStartingIndicator,
} from "./ConversationExecutionState";
import { ModelRouteDecisionCard, type ModelRouteDecision } from "./ModelRouteDecisionCard";
import { ConversationLaunchState } from "./ConversationLaunchState";
import type { ProviderCatalog } from "@dcc/contracts";
import type { WorkspaceMessage } from "./thread-projection";
import {
	AssistantMessage,
	DelegationCard,
	SystemMessage,
	UserMessage,
} from "./message-components";
import { EmptyState } from "./EmptyState";
import {
	latestConversationActivitySignature,
	precedingUserPrompt, precedingUserTurn } from "./conversation-recovery";
import { ConversationTrail } from "./ConversationTrail";
import type { WorkspaceFileReference } from "@/components/workspace-file-reference";
import {
	CONVERSATION_MESSAGE_PAGE_SIZE,
	conversationWindowStart,
	INITIAL_CONVERSATION_MESSAGE_LIMIT,
} from "./conversation-window";
import { conversationStartingPhase, shouldShowConversationStarting, shouldShowInitialConversationStarting } from "./conversation-starting.logic";
import { ComputerUsePreview } from "@/features/computer-use/computer-use-preview";
import { useStableMessages } from "./stable-messages";

/** Bottom spacer (40px) plus the gap kept above an anchored prompt (16px). */
const TURN_ANCHOR_RESERVED_PX = 56;

type AssistantRowContext = {
	workspacePath: string | null;
	planApproved: boolean;
	planReadOnly: boolean;
	sessionId: string | null;
	providers?: ProviderCatalog["providers"];
	providerId?: string | null;
	activeMissionSpecRelativePath: string | null;
	activeMissionSpecHash: string | null;
	autoSaveMissionValidation: boolean;
	canContinue: boolean;
	canFork: boolean;
	canRetry: boolean;
	continueInterrupted: (prompt: string | null) => void;
	forkFromMessage: (messageId: string) => void;
	retryInterrupted: (input: { prompt: string; turnId: string }) => void;
	openPlan: () => void;
	openFileReference?: (reference: WorkspaceFileReference) => void;
};

/**
 * One assistant turn. Memoized on the (structurally shared) message and a
 * context whose handlers are stable, so live frames only re-render the turn
 * that is actually streaming.
 */
const AssistantMessageRow = memo(function AssistantMessageRow({
	message,
	context,
	isLatestAssistant,
	isPlanMessage,
	precedingPrompt,
	precedingTurnId,
}: {
	message: WorkspaceMessage;
	context: AssistantRowContext;
	isLatestAssistant: boolean;
	isPlanMessage: boolean;
	precedingPrompt: string | null;
	precedingTurnId: string | null;
}) {
	const incomplete = message.status?.type === "incomplete";
	const onContinue = useMemo(
		() =>
			isLatestAssistant && incomplete && context.canContinue
				? () => context.continueInterrupted(precedingPrompt)
				: undefined,
		[context, incomplete, isLatestAssistant, precedingPrompt],
	);
	const onFork = useMemo(
		() =>
			context.canFork && message.streaming !== true && !message.status
				? () => context.forkFromMessage(message.id)
				: undefined,
		[context, message.id, message.status, message.streaming],
	);
	const onRetry = useMemo(
		() =>
			isLatestAssistant && incomplete && context.canRetry && precedingTurnId
				? () =>
						context.retryInterrupted({
							prompt: precedingPrompt ?? "",
							turnId: precedingTurnId,
						})
				: undefined,
		[context, incomplete, isLatestAssistant, precedingPrompt, precedingTurnId],
	);
	return (
		<AssistantMessage
			content={message.content}
			streaming={message.streaming}
			createdAt={message.createdAt}
			turnId={message.turnId}
			turnStartedAt={message.turnStartedAt}
			turnEndedAt={message.turnEndedAt}
			status={message.status}
			annotations={message.annotations}
			plan={message.plan ?? null}
			workspacePath={context.workspacePath}
			isPlanContext={isPlanMessage}
			isPlanApproved={isPlanMessage && context.planApproved}
			isPlanReadOnly={isPlanMessage && context.planReadOnly}
			sessionId={context.sessionId}
			providers={context.providers}
			providerId={context.providerId}
			modelId={message.model}
			activeMissionSpecRelativePath={context.activeMissionSpecRelativePath}
			activeMissionSpecHash={context.activeMissionSpecHash}
			autoSaveMissionValidation={context.autoSaveMissionValidation}
			onContinue={onContinue}
			onFork={onFork}
			onRetry={onRetry}
			onOpenPlan={context.openPlan}
			onOpenFileReference={context.openFileReference}
			hidePendingApprovals
		/>
	);
});

type ActiveThreadViewportProps = {
	messages: WorkspaceMessage[];
	hasLoaded: boolean;
	isEmpty: boolean;
	workspaceName: string;
	sessionState: string | null;
	lastTurnState: string | null;
	pendingPrompt: string | null;
	/** True while the header is creating a new session and attaching its runtime. */
	startingSession?: boolean;
	workspacePath: string | null;
	workspaceId?: string | null;
	providers?: ProviderCatalog["providers"];
	providerId?: string | null;
	planMessageId: string | null;
	planApproved: boolean;
	planReadOnly: boolean;
	sessionId: string | null;
	activeMissionSpecRelativePath: string | null;
	activeMissionSpecHash: string | null;
	autoSaveMissionValidation: boolean;
	onSelectSession: (sessionId: string) => void;
	onAbortSession?: () => void;
	/** Reveals the inspector to review the current Git changes. */
	onReviewChanges?: () => void;
	onReviewDelegation?: (delegationId: string) => void;
	onRerunDelegation?: (input: {
		delegationId: string;
		targetProviderId: string;
	}) => Promise<void>;
	/** Puts a finished delegation's result in the parent's composer. */
	onSendDelegationResult?: (input: {
		delegationId: string;
		failureReason?: string | null;
	}) => void | Promise<void>;
	/** "Edit from here": rewind to just before this user message. */
	onEditFromMessage?: (request: EditFromHereRequest) => void;
	onForkFromMessage?: (messageId: string) => void;
	onContinueInterrupted?: (originalPrompt: string | null) => Promise<void> | void;
	onRetryInterrupted?: (input: { prompt: string; turnId: string }) => Promise<void> | void;
	onReviewCompletion?: (input: { prompt: string; turnId: string }) => void;
	onRegenerateCompletion?: (input: { prompt: string; turnId: string }) => Promise<void> | void;
	onKeepCompletion?: (input: { turnId: string; score: number | null }) => void | Promise<void>;
	onOpenPlan: () => void;
	onOpenFileReference?: (reference: WorkspaceFileReference) => void;
	/** Reveals and highlights one message (find-in-thread); nonce re-fires the same id. */
	focusRequest?: { messageId: string; nonce: number } | null;
	completionReviews?: ReadonlyMap<
		string,
		{ needsReview: boolean; score: number | null; kept: boolean; evaluation?: import("@dcc/contracts").DecisionEvaluation | null }
	>;
	completionReviewActionsDismissed?: ReadonlySet<string>;
	isModelRouting?: boolean;
	modelRouteDecision?: ModelRouteDecision | null;
	onResolveModelRoute?: (modelId: string) => void;
	modelLabel?: (modelId: string | null) => string;
};

export function ActiveThreadViewport({
	messages: projectedMessages,
	hasLoaded,
	isEmpty,
	workspaceName,
	lastTurnState,
	pendingPrompt,
	startingSession = false,
	workspacePath,
	workspaceId,
	providers,
	providerId,
	planMessageId,
	planApproved,
	planReadOnly,
	sessionId,
	activeMissionSpecRelativePath,
	activeMissionSpecHash,
	autoSaveMissionValidation,
	onSelectSession,
	onAbortSession,
	onReviewChanges,
	onReviewDelegation,
	onRerunDelegation,
	onSendDelegationResult,
	onEditFromMessage,
	onForkFromMessage,
	onContinueInterrupted,
	onRetryInterrupted,
	onReviewCompletion,
	onRegenerateCompletion,
	onKeepCompletion,
	onOpenPlan,
	onOpenFileReference,
	focusRequest = null,
	completionReviews,
	completionReviewActionsDismissed,
	isModelRouting = false,
	modelRouteDecision = null,
	onResolveModelRoute,
	modelLabel = (modelId) => modelId ?? "—",
}: ActiveThreadViewportProps) {
	const { t } = useTranslation("common");
	const messages = useStableMessages(projectedMessages);
	// Handlers change identity on every parent render; rows read the latest
	// through a ref so memoized turns are not invalidated by them.
	const handlersRef = useRef({
		onContinueInterrupted,
		onForkFromMessage,
		onRetryInterrupted,
		onOpenPlan,
		onOpenFileReference,
	});
	handlersRef.current = {
		onContinueInterrupted,
		onForkFromMessage,
		onRetryInterrupted,
		onOpenPlan,
		onOpenFileReference,
	};
	const continueInterrupted = useCallback((prompt: string | null) => {
		void handlersRef.current.onContinueInterrupted?.(prompt);
	}, []);
	const forkFromMessage = useCallback((messageId: string) => {
		handlersRef.current.onForkFromMessage?.(messageId);
	}, []);
	const retryInterrupted = useCallback((input: { prompt: string; turnId: string }) => {
		void handlersRef.current.onRetryInterrupted?.(input);
	}, []);
	const openPlan = useCallback(() => handlersRef.current.onOpenPlan(), []);
	const openFileReference = useCallback(
		(reference: WorkspaceFileReference) => handlersRef.current.onOpenFileReference?.(reference),
		[],
	);
	const hasOpenFileReference = Boolean(onOpenFileReference);
	const assistantRowContext = useMemo<AssistantRowContext>(
		() => ({
			workspacePath,
			planApproved,
			planReadOnly,
			sessionId,
			providers,
			providerId,
			activeMissionSpecRelativePath,
			activeMissionSpecHash,
			autoSaveMissionValidation,
			canContinue: Boolean(onContinueInterrupted),
			canFork: Boolean(onForkFromMessage),
			canRetry: Boolean(onRetryInterrupted),
			continueInterrupted,
			forkFromMessage,
			retryInterrupted,
			openPlan,
			openFileReference: hasOpenFileReference ? openFileReference : undefined,
		}),
		[
			activeMissionSpecHash,
			activeMissionSpecRelativePath,
			autoSaveMissionValidation,
			continueInterrupted,
			forkFromMessage,
			hasOpenFileReference,
			onContinueInterrupted,
			onForkFromMessage,
			onRetryInterrupted,
			openFileReference,
			openPlan,
			planApproved,
			planReadOnly,
			providerId,
			providers,
			retryInterrupted,
			sessionId,
			workspacePath,
		],
	);
	const [hasNewActivity, setHasNewActivity] = useState(false);
	const [conversationWindow, setConversationWindow] = useState({
		sessionId,
		messageLimit: INITIAL_CONVERSATION_MESSAGE_LIMIT,
	});
	const visibleMessageLimit =
		conversationWindow.sessionId === sessionId
			? conversationWindow.messageLimit
			: INITIAL_CONVERSATION_MESSAGE_LIMIT;
	const prependScrollAnchorRef = useRef<{
		scrollHeight: number;
		scrollTop: number;
	} | null>(null);
	const visibleStart = useMemo(
		() => conversationWindowStart(messages, visibleMessageLimit),
		[messages, visibleMessageLimit],
	);
	const visibleMessages = useMemo(
		() => messages.slice(visibleStart),
		[messages, visibleStart],
	);
	const earlierMessageCount = useMemo(() => {
		if (visibleStart === 0) return 0;
		const nextStart = conversationWindowStart(
			messages,
			visibleMessageLimit + CONVERSATION_MESSAGE_PAGE_SIZE,
		);
		return visibleStart - nextStart;
	}, [messages, visibleMessageLimit, visibleStart]);
	const hiddenUserMessageCount = useMemo(
		() => messages.slice(0, visibleStart).filter((message) => message.role === "user").length,
		[messages, visibleStart],
	);
	const hasStreamingMessage = messages.some(
		(message) => message.role === "assistant" && Boolean(message.streaming),
	);
	const showConversationStarting = shouldShowConversationStarting(
		messages,
		pendingPrompt,
		lastTurnState,
	);
	// Turn anchoring (as in T3 Code and Monocode): a turn sent from this view,
	// or one already running when the thread opens, is pinned near the top
	// with the rest of the viewport reserved below it. Opening a settled
	// thread keeps the usual bottom-aligned history.
	const userMessageCount = useMemo(
		() => messages.reduce((count, message) => count + (message.role === "user" ? 1 : 0), 0),
		[messages],
	);
	const anchorStateRef = useRef<{ sessionId: string | null; baseline: number; active: boolean } | null>(null);
	if (anchorStateRef.current?.sessionId !== sessionId) {
		anchorStateRef.current = {
			sessionId,
			baseline: userMessageCount,
			active: hasStreamingMessage,
		};
	} else if (userMessageCount > anchorStateRef.current.baseline) {
		anchorStateRef.current = { sessionId, baseline: userMessageCount, active: true };
	} else if (userMessageCount < anchorStateRef.current.baseline) {
		// Edit-from-here rewound the thread: re-baseline without anchoring.
		anchorStateRef.current = { sessionId, baseline: userMessageCount, active: false };
	}
	const anchorActive = anchorStateRef.current.active;
	const anchorVisibleIndex = useMemo(() => {
		if (!anchorActive) return -1;
		for (let index = visibleMessages.length - 1; index >= 0; index -= 1) {
			if (visibleMessages[index]?.role === "user") return index;
		}
		return -1;
	}, [anchorActive, visibleMessages]);
	const startingPhase = isModelRouting
		? "routing"
		: conversationStartingPhase(sessionId, lastTurnState);
	const { contentRef, scrollRef, scrollToBottom, isAtBottom, stopScroll } = useStickToBottom({
		initial: "instant",
		// Token-by-token height changes should not start overlapping smooth-scroll
		// animations. Explicit user navigation remains smooth below.
		resize: hasStreamingMessage ? "instant" : "smooth",
	});

	const activitySignature = latestConversationActivitySignature(messages);
	const latestAssistantMessageId = [...messages]
		.reverse()
		.find((message) => message.role === "assistant")?.id;
	const previousActivityRef = useRef<string | null>(null);
	const wasAtBottomRef = useRef(true);

	useEffect(() => {
		prependScrollAnchorRef.current = null;
		previousActivityRef.current = activitySignature;
		wasAtBottomRef.current = true;
		setHasNewActivity(false);
		void scrollToBottom("instant");
		// A session switch is the only time the viewport intentionally resets.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [sessionId]);

	useLayoutEffect(() => {
		const anchor = prependScrollAnchorRef.current;
		const scrollElement = scrollRef.current;
		if (!anchor || !scrollElement) return;
		prependScrollAnchorRef.current = null;
		scrollElement.scrollTop =
			anchor.scrollTop + (scrollElement.scrollHeight - anchor.scrollHeight);
	}, [scrollRef, visibleStart]);

	useEffect(() => {
		const previous = previousActivityRef.current;
		previousActivityRef.current = activitySignature;
		if (previous !== null && previous !== activitySignature && !wasAtBottomRef.current) {
			setHasNewActivity(true);
		}
	}, [activitySignature]);

	useEffect(() => {
		wasAtBottomRef.current = isAtBottom;
		if (isAtBottom) {
			setHasNewActivity(false);
		}
	}, [isAtBottom]);

	const threadSurfaceVisible =
		!isModelRouting &&
		hasLoaded &&
		!(isEmpty || messages.length === 0) &&
		!shouldShowInitialConversationStarting(messages, pendingPrompt, lastTurnState);

	// A new turn sent from here scrolls its prompt to the anchor, even when
	// the person was reading older messages.
	const anchorBaseline = anchorStateRef.current.baseline;
	const previousAnchorBaselineRef = useRef(anchorBaseline);
	useLayoutEffect(() => {
		const previous = previousAnchorBaselineRef.current;
		previousAnchorBaselineRef.current = anchorBaseline;
		if (anchorActive && anchorBaseline > previous) {
			setHasNewActivity(false);
			void scrollToBottom("smooth");
		}
	}, [anchorActive, anchorBaseline, scrollToBottom]);

	// The reserved space is the visible height minus the bottom spacer and a
	// small top inset. The viewport also shrinks when the composer dock grows
	// (an agent offer, a multi-line draft); stay at the bottom if we were.
	useEffect(() => {
		const scrollElement = scrollRef.current;
		const content = contentRef.current;
		if (!scrollElement || !content || typeof ResizeObserver === "undefined") return;
		let previousHeight = scrollElement.clientHeight;
		const apply = () => {
			const height = scrollElement.clientHeight;
			content.style.setProperty(
				"--dcc-turn-anchor-min-height",
				`${Math.max(0, height - TURN_ANCHOR_RESERVED_PX)}px`,
			);
			if (height < previousHeight && wasAtBottomRef.current) {
				void scrollToBottom("instant");
			}
			previousHeight = height;
		};
		apply();
		const observer = new ResizeObserver(apply);
		observer.observe(scrollElement);
		return () => observer.disconnect();
		// The scroll area only mounts once there is a thread to show.
	}, [contentRef, scrollRef, scrollToBottom, threadSurfaceVisible]);

	// When the running turn settles its activity folds; if the person was
	// following, keep the end of the answer in view.
	const followingWhileStreamingRef = useRef(false);
	if (hasStreamingMessage) followingWhileStreamingRef.current = wasAtBottomRef.current;
	const previousStreamingRef = useRef(hasStreamingMessage);
	useLayoutEffect(() => {
		const settled = previousStreamingRef.current && !hasStreamingMessage;
		previousStreamingRef.current = hasStreamingMessage;
		if (settled && followingWhileStreamingRef.current) {
			void scrollToBottom("instant");
		}
	}, [hasStreamingMessage, scrollToBottom]);

	const handleScrollToBottom = useCallback(() => {
		setHasNewActivity(false);
		void scrollToBottom("smooth");
	}, [scrollToBottom]);

	// Find-in-thread: make the target visible (the window only grows, never
	// shrinks) and scroll it into view once it is rendered.
	const focusedMessageId = focusRequest?.messageId ?? null;
	useEffect(() => {
		if (!focusRequest) return;
		const index = messages.findIndex((message) => message.id === focusRequest.messageId);
		if (index < 0) return;
		if (index < visibleStart) {
			setConversationWindow({ sessionId, messageLimit: messages.length });
		}
	}, [focusRequest, messages, sessionId, visibleStart]);
	useLayoutEffect(() => {
		if (!focusRequest) return;
		const scrollElement = scrollRef.current;
		if (!scrollElement) return;
		const target = scrollElement.querySelector<HTMLElement>(
			`[data-conversation-trail-id="${CSS.escape(focusRequest.messageId)}"]`,
		);
		target?.scrollIntoView({ block: "center" });
	}, [focusRequest, scrollRef, visibleStart]);

	const handleLoadEarlier = useCallback(() => {
		const scrollElement = scrollRef.current;
		if (scrollElement) {
			prependScrollAnchorRef.current = {
				scrollHeight: scrollElement.scrollHeight,
				scrollTop: scrollElement.scrollTop,
			};
		}
		setConversationWindow((current) => ({
			sessionId,
			messageLimit:
				(current.sessionId === sessionId
					? current.messageLimit
					: INITIAL_CONVERSATION_MESSAGE_LIMIT) + CONVERSATION_MESSAGE_PAGE_SIZE,
		}));
	}, [scrollRef, sessionId]);

	if (modelRouteDecision && onResolveModelRoute) {
		return (
			<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
				<ModelRouteDecisionCard
					decision={modelRouteDecision}
					modelLabel={modelLabel}
					onSelect={onResolveModelRoute}
				/>
			</div>
		);
	}

	if (startingSession) {
		return (
			<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
				<div className="flex min-h-full flex-1 flex-col px-5 py-6">
					<SessionStartingIndicator />
				</div>
			</div>
		);
	}

	if (isModelRouting) {
		return (
			<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
				<ConversationExecutionState phase="routing" />
			</div>
		);
	}

	if (shouldShowInitialConversationStarting(messages, pendingPrompt, lastTurnState)) {
		return (
			<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
				<ConversationExecutionState phase={startingPhase} />
			</div>
		);
	}

	if (!hasLoaded) {
		return (
			<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
				{pendingPrompt ? (
					<ConversationExecutionState phase={startingPhase} />
				) : (
					<EmptyState
						title={t("conversation.loading.title")}
						description={t("conversation.loading.description")}
					/>
				)}
			</div>
		);
	}

	if (isEmpty || messages.length === 0) {
		if (
			pendingPrompt ||
			lastTurnState === "running"
		) {
			return (
				<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
					<ConversationExecutionState phase={startingPhase} />
				</div>
			);
		}

		return (
			<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
				<ConversationLaunchState workspaceName={workspaceName} />
			</div>
		);
	}

	return (
		<div className="dcc-conversation-scroll-area relative min-h-0 flex-1 overflow-hidden">
			<div
				ref={scrollRef}
				tabIndex={0}
				className="dcc-conversation-scroll-viewport h-full w-full overflow-x-hidden overflow-y-auto overscroll-none scrollbar-stable"
			>
				<div ref={contentRef} className="flex min-h-full min-w-0 flex-col">
					<div className="h-6 shrink-0" aria-hidden />
					{visibleStart > 0 ? (
						<div className="flex justify-center px-5 pb-5">
							<Button
								type="button"
								variant="outline"
								size="sm"
								onClick={handleLoadEarlier}
							>
								{t("conversation.loadEarlier", {
									count: earlierMessageCount,
								})}
							</Button>
						</div>
					) : null}
						<div className="dcc-conversation-thread-list flex flex-col gap-0 px-5">
							{(() => {
								const renderMessage = (message: WorkspaceMessage, visibleMessageIndex: number) => {
								const messageIndex = visibleStart + visibleMessageIndex;
								if (message.role === "user") {
									return (
										<div
											key={message.id}
											data-conversation-trail-id={message.id}
											className={cn(
												"scroll-mt-6 pb-4",
												focusedMessageId === message.id && "dcc-thread-find-focus",
											)}
										>
											<UserMessage
												label={message.label}
												content={message.content}
												createdAt={message.createdAt}
												evidence={message.evidence ?? null}
												retryOfTurnId={message.retryOfTurnId ?? null}
												onFork={
													onForkFromMessage
														? () => onForkFromMessage(message.id)
														: undefined
												}
												onEdit={
													onEditFromMessage
														? () =>
																onEditFromMessage({
																	messageId: message.id,
																	turnId: message.turnId ?? null,
																	prompt: message.content,
																})
														: undefined
												}
											/>
										</div>
									);
								}
								if (message.role === "assistant") {
									const completionReview = message.turnId
										? completionReviews?.get(message.turnId)
										: undefined;
									const sourceTurn = message.turnId
										? precedingUserTurn(messages, messageIndex)
										: null;
									return (
										<div
											key={message.id}
											data-conversation-trail-id={message.id}
											className={cn(
												"scroll-mt-6 pb-4",
												focusedMessageId === message.id && "dcc-thread-find-focus",
											)}
										>
											<AssistantMessageRow
												message={message}
												context={assistantRowContext}
												isLatestAssistant={message.id === latestAssistantMessageId}
												isPlanMessage={message.id === planMessageId}
												precedingPrompt={precedingUserPrompt(messages, messageIndex)}
												precedingTurnId={
													precedingUserTurn(messages, messageIndex)?.turnId ?? null
												}
											/>
											{completionReview && message.turnSettled && !message.streaming ? (
												<div className="mt-2 flex flex-wrap items-center gap-2">
													<Badge
														variant={completionReview.needsReview ? "destructive" : "secondary"}
														className="text-[10px] font-normal"
													>
										{completionReview.kept
											? t("settings.decisionProvider.completionKeptLabel")
											: completionReview.needsReview
											? t("settings.decisionProvider.completionNeedsReview")
											: t("settings.decisionProvider.completionComplete")}
														{completionReview.score !== null
															? ` · ${t("settings.decisionProvider.reviewSignal", { score: Math.round(completionReview.score * 100) })}`
															: ""}
													</Badge>
												<DecisionEvaluationDetails evaluation={completionReview.evaluation} />
					{completionReview.needsReview && !completionReview.kept && sourceTurn?.turnId && onReviewCompletion && !completionReviewActionsDismissed?.has(message.turnId ?? "") ? (
														<Button
															type="button"
															variant="ghost"
															size="xs"
															className="h-6 px-2 text-[11px]"
															onClick={() =>
																onReviewCompletion({
																	prompt: sourceTurn.prompt,
																	turnId: sourceTurn.turnId!,
																})
															}
														>
															{t("settings.decisionProvider.completionReviewAction")}
														</Button>
													) : null}
					{completionReview.needsReview && !completionReview.kept && sourceTurn?.turnId && onRegenerateCompletion && !completionReviewActionsDismissed?.has(message.turnId ?? "") ? (
														<Button
															type="button"
															variant="ghost"
															size="xs"
															className="h-6 px-2 text-[11px]"
															onClick={() =>
																void onRegenerateCompletion({
																	prompt: sourceTurn.prompt,
																	turnId: sourceTurn.turnId!,
																})
															}
														>
															{t("settings.decisionProvider.completionRegenerateAction")}
														</Button>
					) : null}
					{completionReview.needsReview && !completionReview.kept && sourceTurn?.turnId && onKeepCompletion && !completionReviewActionsDismissed?.has(message.turnId ?? "") ? (
						<Button
							type="button"
							variant="ghost"
							size="xs"
							className="h-6 px-2 text-[11px]"
							onClick={() =>
								void onKeepCompletion({
									turnId: sourceTurn.turnId!,
									score: completionReview.score,
								})
							}
						>
							{t("settings.decisionProvider.completionKeepAction")}
						</Button>
					) : null}
												</div>
											) : null}
											{message.turnSettled && message.turnId && sessionId && workspaceId && onReviewChanges ? (
												<TurnReviewTimelineCard
													key={`${sessionId}:${message.turnId}:${workspaceId}`}
													target={{ sessionId, workspaceId, turnId: message.turnId }}
													workspaceRoot={workspacePath}
													onReview={onReviewChanges}
													onInteraction={stopScroll}
												/>
											) : null}
										</div>
									);
								}
								if (message.delegation) {
									return (
										<div key={message.id} data-conversation-trail-id={message.id} className="scroll-mt-6 pb-4">
											<DelegationCard
												delegation={message.delegation}
												fallbackContent={message.content}
												createdAt={message.createdAt}
												workspaceId={workspaceId ?? null}
												providers={providers ?? []}
												onSelectSession={onSelectSession}
												onReviewChanges={onReviewChanges}
												onReviewDelegation={onReviewDelegation}
												onRerunDelegation={onRerunDelegation}
												onSendDelegationResult={onSendDelegationResult}
											/>
										</div>
									);
								}
								return (
									<div
										key={message.id}
										data-conversation-trail-id={message.id}
										className={cn(
											"scroll-mt-6 pb-4",
											focusedMessageId === message.id && "dcc-thread-find-focus",
										)}
									>
										<SystemMessage
											label={message.label}
											content={message.content}
											createdAt={message.createdAt}
											action={
												message.action?.type === "open-session"
													? {
															label: message.action.label,
															onClick: () =>
																onSelectSession(message.action!.sessionId),
														}
													: undefined
											}
										/>
									</div>
								);
							};
								const startingIndicator = showConversationStarting ? (
								<div className="pb-4">
									<ConversationStartingIndicator phase={startingPhase} />
								</div>
							) : null;
								if (anchorVisibleIndex < 0) {
									return (
										<>
											{visibleMessages.map(renderMessage)}
											{startingIndicator}
										</>
									);
								}
								// The turn sent from this view is pinned near the top
								// and reserves the rest of the viewport, so its reply
								// grows (and later folds) without moving the page.
								return (
									<>
										{visibleMessages
											.slice(0, anchorVisibleIndex)
											.map((message, index) => renderMessage(message, index))}
										<div className="dcc-turn-anchor" data-turn-anchor>
											{visibleMessages
												.slice(anchorVisibleIndex)
												.map((message, index) =>
													renderMessage(message, anchorVisibleIndex + index),
												)}
											{startingIndicator}
										</div>
									</>
								);
							})()}
						</div>
					<div className="h-10 shrink-0" aria-hidden />
				</div>
			</div>
			<ConversationTrail
				messages={visibleMessages}
				scrollRef={scrollRef}
				ordinalOffset={hiddenUserMessageCount}
			/>
			{!isAtBottom ? (
				<button
					type="button"
					className="conversation-scroll-jump"
					data-new-activity={hasNewActivity ? "true" : "false"}
					aria-label={
						hasNewActivity
							? t("conversation.newActivity")
							: t("conversation.scrollToBottom")
					}
					title={
						hasNewActivity
							? t("conversation.newActivity")
							: t("conversation.scrollToBottom")
					}
					onClick={handleScrollToBottom}
				>
					<ChevronDown className="size-4" strokeWidth={2} />
				</button>
			) : null}
			<ComputerUsePreview
				sessionId={sessionId}
				isTurnActive={lastTurnState === "running"}
				onStop={onAbortSession ?? (() => undefined)}
			/>
		</div>
	);
}
