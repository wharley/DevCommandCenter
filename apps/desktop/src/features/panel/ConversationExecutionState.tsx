import { useTranslation } from "react-i18next";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { UserMessage } from "./message-components";

type ConversationExecutionStateProps = {
	pendingPrompt: string | null;
};

/**
 * Optimistic first-turn timeline shown while the provider session boots and
 * before persisted events catch up. It deliberately mirrors the real chat so
 * the user never falls into an infrastructure/loading screen after sending.
 */
export function ConversationExecutionState({
	pendingPrompt,
}: ConversationExecutionStateProps) {
	const { t } = useTranslation("common");

	return (
		<div className="flex min-h-full flex-1 flex-col px-5 py-6">
			{pendingPrompt ? (
				<div className="pb-4">
					<UserMessage
						label={t("conversation.roles.user")}
						content={pendingPrompt}
					/>
				</div>
			) : null}
			<div className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-start">
				<div className="flex min-w-0 items-center gap-2 py-2 text-[13px] text-muted-foreground">
					<DccThinkingIndicator size={15} />
					<span>{t("conversation.starting")}</span>
				</div>
			</div>
		</div>
	);
}
