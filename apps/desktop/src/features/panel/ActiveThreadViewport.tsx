import { useEffect, useMemo, useRef, useState } from "react";
import type { CoreEvent } from "@dcc/contracts";
import { projectWorkspaceMessages } from "./thread-projection";
import { AssistantMessage, SystemMessage, UserMessage } from "./message-components";
import { EmptyState } from "./EmptyState";

type ActiveThreadViewportProps = {
	events: CoreEvent[];
	hasLoaded: boolean;
	isEmpty: boolean;
};

export function ActiveThreadViewport({
	events,
	hasLoaded,
	isEmpty,
}: ActiveThreadViewportProps) {
	const scrollRef = useRef<HTMLDivElement | null>(null);
	const [stickToBottom, setStickToBottom] = useState(true);
	const messages = useMemo(() => projectWorkspaceMessages(events), [events]);

	useEffect(() => {
		const container = scrollRef.current;
		if (!container || !stickToBottom) {
			return;
		}

		container.scrollTop = container.scrollHeight;
	}, [messages.length, stickToBottom]);

	useEffect(() => {
		const container = scrollRef.current;
		if (!container) {
			return;
		}

		const updateStickiness = () => {
			const remaining =
				container.scrollHeight - container.scrollTop - container.clientHeight;
			setStickToBottom(remaining < 40);
		};

		updateStickiness();
		container.addEventListener("scroll", updateStickiness, { passive: true });
		window.addEventListener("resize", updateStickiness);
		return () => {
			container.removeEventListener("scroll", updateStickiness);
			window.removeEventListener("resize", updateStickiness);
		};
	}, []);

	if (!hasLoaded || isEmpty) {
		return (
			<EmptyState
				title="No active session"
				description="Start a session from the composer to render the thread viewport here."
			/>
		);
	}

	return (
		<div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
			<div
				ref={scrollRef}
				className="conversation-scrollbar-fade-in relative flex min-h-0 flex-1 flex-col overflow-y-auto overflow-x-hidden px-5 py-4"
			>
				<div className="flex min-h-full flex-1 flex-col gap-4">
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
									/>
								);
							}
							if (message.role === "assistant") {
								return (
									<AssistantMessage
										key={message.id}
										content={message.content}
										streaming={message.streaming}
									/>
								);
							}
								return (
									<SystemMessage
										key={message.id}
										label={message.label}
										content={message.content}
									/>
								);
						})
					)}
				</div>
			</div>
		</div>
	);
}
