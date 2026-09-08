import type { RawSessionEvent } from "./threadEvents";
export type AgentQuestion = {
	id: string;
	header: string;
	question: string;
	options: { label: string; description: string }[];
};
export type QuestionRequest = {
	requestId: string;
	questions: AgentQuestion[];
	at: string;
};
export function pendingQuestions(events: RawSessionEvent[]): QuestionRequest[] {
	const pending = new Map<string, QuestionRequest>();
	for (const event of [...events].sort((a, b) => a.sequence - b.sequence)) {
		const kind = event.kind;
		const id = kind.requestId;
		if (!id) continue;
		if (
			kind.type === "turn_user_input_requested" &&
			Array.isArray(kind.questions)
		) {
			pending.set(id, {
				requestId: id,
				questions: kind.questions as AgentQuestion[],
				at: event.occurredAt,
			});
		} else if (kind.type === "turn_user_input_resolved") pending.delete(id);
	}
	return [...pending.values()];
}
