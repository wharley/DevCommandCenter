import { useCallback, useEffect } from "react";
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
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";
import type { AgentInitiatedDelegationRequest } from "@/features/sessions/agent-delegation-request";

type ActiveThreadViewportProps = {
	messages: WorkspaceMessage[];
	hasLoaded: boolean;
	isEmpty: boolean;
	workspaceName: string;
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	sessionState: string | null;
	lastTurnState: string | null;
	pendingPrompt: string | null;
	workspacePath: string | null;
	workspaceId?: string | null;
	providers?: ProviderCatalog["providers"];
	planMessageId: string | null;
	sessionId: string | null;
	activeMissionSpecRelativePath: string | null;
	activeMissionSpecHash: string | null;
	autoSaveMissionValidation: boolean;
	onStartSession: () => void;
	onSelectSession: (sessionId: string) => void;
	onSubmitPrompt: (
		turn: ComposerSubmittedTurn,
		options?: { forceNewSession?: boolean; targetSessionId?: string | null },
	) => Promise<void>;
	/** Reveals the inspector to review a turn's edits ([Revisar] card). */
	onReviewChanges?: () => void;
	onReviewDelegation?: (delegationId: string) => void;
	onDelegateTaskApprove?: (request: AgentInitiatedDelegationRequest) => Promise<void>;
};

export function ActiveThreadViewport({
	messages,
	hasLoaded,
	isEmpty,
	workspaceName,
	selectedProviderLabel,
	selectedModelLabel,
	sessionState,
	lastTurnState,
	pendingPrompt,
	workspacePath,
	workspaceId,
	providers,
	planMessageId,
	sessionId,
	activeMissionSpecRelativePath,
	activeMissionSpecHash,
	autoSaveMissionValidation,
	onStartSession,
	onSelectSession,
	onSubmitPrompt,
	onReviewChanges,
	onReviewDelegation,
	onDelegateTaskApprove,
}: ActiveThreadViewportProps) {
	const { t } = useTranslation("common");
	const { contentRef, scrollRef, scrollToBottom, isAtBottom } = useStickToBottom({
		initial: "instant",
		resize: "smooth",
	});

	const latestMessageKey = messages[messages.length - 1]?.id ?? "empty";

	useEffect(() => {
		void scrollToBottom("instant");
	// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [latestMessageKey]);

	const handleScrollToBottom = useCallback(() => {
		void scrollToBottom("smooth");
	}, [scrollToBottom]);

	if (!hasLoaded) {
		return (
			<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
				<EmptyState
					title="Loading conversation"
					description="Preparing the thread history for this workspace."
				/>
			</div>
		);
	}

	if (isEmpty) {
		if (sessionState === "active" || lastTurnState === "running") {
			return (
				<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
					<ConversationExecutionState
						workspaceName={workspaceName}
						selectedProviderLabel={selectedProviderLabel}
						selectedModelLabel={selectedModelLabel}
						sessionState={sessionState}
						lastTurnState={lastTurnState}
						pendingPrompt={pendingPrompt}
					/>
				</div>
			);
		}

		return (
			<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
				<ConversationLaunchState
					workspaceName={workspaceName}
					selectedProviderLabel={selectedProviderLabel}
					selectedModelLabel={selectedModelLabel}
					onStartSession={onStartSession}
					onSubmitPrompt={onSubmitPrompt}
				/>
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
							{messages.map((message) => {
								if (message.role === "user") {
									return (
										<div key={message.id} className="pb-4">
											<UserMessage
												label={message.label}
												content={message.content}
												createdAt={message.createdAt}
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
												sessionId={sessionId}
												activeMissionSpecRelativePath={activeMissionSpecRelativePath}
												activeMissionSpecHash={activeMissionSpecHash}
												autoSaveMissionValidation={autoSaveMissionValidation}
												onReviewChanges={onReviewChanges}
												onDelegateTaskApprove={onDelegateTaskApprove}
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
						{t("conversation.scrollToBottom")}
					</Button>
				</div>
			) : null}
		</div>
	);
}
