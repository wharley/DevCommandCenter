import type { WorkspaceMessage } from "./thread-projection";

export type PlanFollowUpState = {
	latestPlanMessage: WorkspaceMessage | null;
	activePlanMessage: WorkspaceMessage | null;
	showPlanFollowUpPrompt: boolean;
};

export function derivePlanFollowUpState(
	messages: WorkspaceMessage[],
): PlanFollowUpState {
	const latestPlanIndex = findLatestPlanMessageIndex(messages);
	if (latestPlanIndex < 0) {
		return {
			latestPlanMessage: null,
			activePlanMessage: null,
			showPlanFollowUpPrompt: false,
		};
	}

	const latestPlanMessage = messages[latestPlanIndex] ?? null;
	const hasLaterConversation = messages
		.slice(latestPlanIndex + 1)
		.some(
			(message) =>
				message.role === "user" || message.role === "assistant",
		);
	const activePlanMessage = hasLaterConversation ? null : latestPlanMessage;
	const showPlanFollowUpPrompt = Boolean(
		activePlanMessage &&
			!activePlanMessage.streaming &&
			activePlanMessage.status == null,
	);

	return {
		latestPlanMessage,
		activePlanMessage,
		showPlanFollowUpPrompt,
	};
}

function findLatestPlanMessageIndex(messages: WorkspaceMessage[]) {
	for (let index = messages.length - 1; index >= 0; index -= 1) {
		const message = messages[index];
		if (message?.role === "assistant" && message.plan?.isPlanLike) {
			return index;
		}
	}
	return -1;
}
