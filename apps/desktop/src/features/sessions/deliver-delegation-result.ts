import type { SendTurnInput } from "@dcc/contracts";
import { queueTurn, sendTurn } from "@/lib/session-api";

type DeliveryApi = {
	queueTurn: (input: { turn: SendTurnInput }) => Promise<unknown>;
	sendTurn: (input: SendTurnInput) => Promise<unknown>;
};

/**
 * Wakes the parent agent with a delegation result.
 *
 * While the parent still has an open turn the result is queued, and the
 * backend dispatches it right after that turn completes. The queue only
 * accepts follow-ups during an open turn, so an idle parent gets the result
 * as a new turn instead. Either way the person can see it in the thread.
 */
export async function deliverDelegationResultToParent(
	input: {
		parentSessionId: string;
		prompt: string;
		toolInstructions: string | null;
	},
	api: DeliveryApi = { queueTurn, sendTurn },
): Promise<"queued" | "sent"> {
	const turn: SendTurnInput = {
		sessionId: input.parentSessionId,
		prompt: input.prompt,
		toolInstructions: input.toolInstructions,
		providerId: null,
		model: null,
		providerRuntime: null,
		planMode: null,
		effort: null,
		fastMode: null,
		approvalPolicy: null,
	};
	try {
		await api.queueTurn({ turn });
		return "queued";
	} catch {
		await api.sendTurn(turn);
		return "sent";
	}
}
