import { ChevronDown } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { ConversationExecutionState } from "./ConversationExecutionState";
import { ConversationLaunchState } from "./ConversationLaunchState";
import type { WorkspaceMessage } from "./thread-projection";
import { AssistantMessage, SystemMessage, UserMessage } from "./message-components";
import { EmptyState } from "./EmptyState";
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";

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
	planMessageId: string | null;
	onStartSession: () => void;
	onSubmitPrompt: (turn: ComposerSubmittedTurn) => Promise<void>;
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
	planMessageId,
	onStartSession,
	onSubmitPrompt,
}: ActiveThreadViewportProps) {
	const scrollRef = useRef<HTMLDivElement | null>(null);
	const contentRef = useRef<HTMLDivElement | null>(null);
	const stickToBottomRef = useRef(true);
	const [stickToBottom, setStickToBottom] = useState(true);
	const [showScrollToLatest, setShowScrollToLatest] = useState(false);
	const latestMessageKey = messages[messages.length - 1]?.id ?? "empty";

	useEffect(() => {
		stickToBottomRef.current = stickToBottom;
	}, [stickToBottom]);

	// Scroll to bottom when content grows during streaming
	useEffect(() => {
		const container = scrollRef.current;
		const content = contentRef.current;
		if (!container || !content) return;

		const observer = new ResizeObserver(() => {
			if (stickToBottomRef.current) {
				container.scrollTop = container.scrollHeight;
				setShowScrollToLatest(false);
			}
		});

		observer.observe(content);
		return () => observer.disconnect();
	}, []);

	useEffect(() => {
		const container = scrollRef.current;
		if (!container || !stickToBottom) {
			return;
		}

		container.scrollTop = container.scrollHeight;
		setShowScrollToLatest(false);
	}, [latestMessageKey, stickToBottom]);

	useEffect(() => {
		const container = scrollRef.current;
		if (!container) {
			return;
		}

		const updateStickiness = () => {
			const remaining =
				container.scrollHeight - container.scrollTop - container.clientHeight;
			const anchored = remaining < 40;
			setStickToBottom(anchored);
			setShowScrollToLatest(!anchored && container.scrollHeight > container.clientHeight);
		};

		updateStickiness();
		container.addEventListener("scroll", updateStickiness, { passive: true });
		window.addEventListener("resize", updateStickiness);
		return () => {
			container.removeEventListener("scroll", updateStickiness);
			window.removeEventListener("resize", updateStickiness);
		};
	}, []);

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
				className="dcc-conversation-scroll-viewport conversation-scrollbar-fade-in h-full w-full overflow-x-hidden overflow-y-auto overscroll-none px-5 py-4 scrollbar-stable"
			>
				<div ref={contentRef} className="flex min-h-full min-w-0 flex-col justify-end gap-4">
					{messages.length === 0 ? (
						<EmptyState
							title="Session loaded"
							description="The timeline is still empty. Send a prompt to begin the conversation."
						/>
					) : (
						messages.map((message) => {
							if (message.role === "user") {
								return (
									<UserMessage
										key={message.id}
										label={message.label}
										content={message.content}
										createdAt={message.createdAt}
									/>
								);
							}
							if (message.role === "assistant") {
								return (
									<AssistantMessage
										key={message.id}
										content={message.content}
										streaming={message.streaming}
										createdAt={message.createdAt}
										status={message.status}
										annotations={message.annotations}
										plan={message.plan ?? null}
										workspacePath={workspacePath}
										isPlanContext={message.id === planMessageId}
									/>
								);
							}
							return (
								<SystemMessage
									key={message.id}
									label={message.label}
									content={message.content}
									createdAt={message.createdAt}
								/>
							);
						})
					)}
					<div className="h-10 shrink-0" aria-hidden />
				</div>
			</div>
			{showScrollToLatest ? (
				<div className="pointer-events-none absolute inset-x-0 bottom-1 z-30 flex justify-center py-1.5">
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="conversation-scroll-button pointer-events-auto"
						onClick={() => {
							const container = scrollRef.current;
							if (!container) {
								return;
							}
							container.scrollTo({
								top: container.scrollHeight,
								behavior: "smooth",
							});
						}}
					>
						<ChevronDown className="size-3.5" strokeWidth={2} />
						Scroll to bottom
					</Button>
				</div>
			) : null}
		</div>
	);
}
