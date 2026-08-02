import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import { useStickToBottom } from "use-stick-to-bottom";
import { Button } from "@/components/ui/button";
import { ConversationExecutionState } from "./ConversationExecutionState";
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
import type { AgentInitiatedDelegationRequest } from "@/features/sessions/agent-delegation-request";
import {
	latestConversationActivitySignature,
	precedingUserPrompt,
} from "./conversation-recovery";

type ActiveThreadViewportProps = {
	messages: WorkspaceMessage[];
	hasLoaded: boolean;
	isEmpty: boolean;
	workspaceName: string;
	sessionState: string | null;
	lastTurnState: string | null;
	pendingPrompt: string | null;
	workspacePath: string | null;
	workspaceId?: string | null;
	providers?: ProviderCatalog["providers"];
	planMessageId: string | null;
	planApproved: boolean;
	planReadOnly: boolean;
	sessionId: string | null;
	activeMissionSpecRelativePath: string | null;
	activeMissionSpecHash: string | null;
	autoSaveMissionValidation: boolean;
	onSelectSession: (sessionId: string) => void;
	/** Reveals the inspector to review the current Git changes. */
	onReviewChanges?: () => void;
	onReviewDelegation?: (delegationId: string) => void;
	onRerunDelegation?: (input: {
		delegationId: string;
		targetProviderId: string;
	}) => Promise<void>;
	onDelegateTaskApprove?: (request: AgentInitiatedDelegationRequest) => Promise<void>;
	onEditPrompt?: (prompt: string) => void;
	onContinueInterrupted?: (originalPrompt: string | null) => Promise<void> | void;
	onOpenPlan: () => void;
};

export function ActiveThreadViewport({
	messages,
	hasLoaded,
	isEmpty,
	workspaceName,
	sessionState,
	lastTurnState,
	pendingPrompt,
	workspacePath,
	workspaceId,
	providers,
	planMessageId,
	planApproved,
	planReadOnly,
	sessionId,
	activeMissionSpecRelativePath,
	activeMissionSpecHash,
	autoSaveMissionValidation,
	onSelectSession,
	onReviewChanges,
	onReviewDelegation,
	onRerunDelegation,
	onDelegateTaskApprove,
	onEditPrompt,
	onContinueInterrupted,
	onOpenPlan,
}: ActiveThreadViewportProps) {
	const { t } = useTranslation("common");
	const [hasNewActivity, setHasNewActivity] = useState(false);
	const { contentRef, scrollRef, scrollToBottom, isAtBottom } = useStickToBottom({
		initial: "instant",
		resize: "smooth",
	});

	const activitySignature = latestConversationActivitySignature(messages);
	const latestAssistantMessageId = [...messages]
		.reverse()
		.find((message) => message.role === "assistant")?.id;
	const previousActivityRef = useRef<string | null>(null);
	const wasAtBottomRef = useRef(true);

	useEffect(() => {
		previousActivityRef.current = activitySignature;
		wasAtBottomRef.current = true;
		setHasNewActivity(false);
		void scrollToBottom("instant");
		// A session switch is the only time the viewport intentionally resets.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [sessionId]);

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

	const handleScrollToBottom = useCallback(() => {
		setHasNewActivity(false);
		void scrollToBottom("smooth");
	}, [scrollToBottom]);

	if (!hasLoaded) {
		return (
			<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
				<EmptyState
					title={t("conversation.loading.title")}
					description={t("conversation.loading.description")}
				/>
			</div>
		);
	}

	if (isEmpty) {
		if (
			pendingPrompt ||
			sessionState === "active" ||
			lastTurnState === "running"
		) {
			return (
				<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
					<ConversationExecutionState pendingPrompt={pendingPrompt} />
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
				className="dcc-conversation-scroll-viewport conversation-scrollbar-fade-in h-full w-full overflow-x-hidden overflow-y-auto overscroll-none scrollbar-stable"
			>
				<div ref={contentRef} className="flex min-h-full min-w-0 flex-col">
					<div className="h-6 shrink-0" aria-hidden />
					{messages.length === 0 ? (
						<div className="flex min-h-full flex-1 items-center justify-center px-8">
							<EmptyState
								title="Session loaded"
								description="The timeline is still empty. Send a prompt to begin the conversation."
							/>
						</div>
					) : (
						<div className="flex flex-col gap-0 px-5">
							{messages.map((message, messageIndex) => {
								if (message.role === "user") {
									return (
										<div key={message.id} className="pb-4">
											<UserMessage
												label={message.label}
												content={message.content}
												createdAt={message.createdAt}
												onEdit={
													onEditPrompt
														? () => onEditPrompt(message.content)
														: undefined
												}
											/>
										</div>
									);
								}
								if (message.role === "assistant") {
									return (
										<div key={message.id} className="pb-4">
											<AssistantMessage
												content={message.content}
												streaming={message.streaming}
												createdAt={message.createdAt}
												status={message.status}
												annotations={message.annotations}
												plan={message.plan ?? null}
												workspacePath={workspacePath}
												isPlanContext={message.id === planMessageId}
												isPlanApproved={
													message.id === planMessageId && planApproved
												}
												isPlanReadOnly={
													message.id === planMessageId && planReadOnly
												}
												sessionId={sessionId}
												activeMissionSpecRelativePath={activeMissionSpecRelativePath}
												activeMissionSpecHash={activeMissionSpecHash}
												autoSaveMissionValidation={autoSaveMissionValidation}
												onDelegateTaskApprove={onDelegateTaskApprove}
												onContinue={
													message.id === latestAssistantMessageId &&
													message.status?.type === "incomplete" &&
													onContinueInterrupted
														? () => {
																void onContinueInterrupted(
																	precedingUserPrompt(messages, messageIndex),
																);
															}
														: undefined
												}
												onOpenPlan={onOpenPlan}
												hidePendingApprovals
											/>
										</div>
									);
								}
								if (message.delegation) {
									return (
										<div key={message.id} className="pb-4">
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
											/>
										</div>
									);
								}
								return (
									<div key={message.id} className="pb-4">
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
							})}
						</div>
					)}
					<div className="h-10 shrink-0" aria-hidden />
				</div>
			</div>
			{!isAtBottom ? (
				<div className="pointer-events-none absolute inset-x-0 bottom-1 z-30 flex justify-center py-1.5">
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="conversation-scroll-button pointer-events-auto"
						onClick={handleScrollToBottom}
					>
						<ChevronDown className="size-3.5" strokeWidth={2} />
						{hasNewActivity
							? t("conversation.newActivity")
							: t("conversation.scrollToBottom")}
					</Button>
				</div>
			) : null}
		</div>
	);
}
