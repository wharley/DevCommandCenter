import type { RawSessionEvent } from "./threadEvents.ts";
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

/** The questions of a request as the desktop serves them, the malformed and blank ones left out. */
export function parseAgentQuestions(raw: unknown): AgentQuestion[] {
	if (!Array.isArray(raw)) return [];
	return raw.flatMap((item): AgentQuestion[] => {
		if (!item || typeof item !== "object") return [];
		const value = item as Record<string, unknown>;
		const question = typeof value.question === "string" ? value.question : "";
		if (!question.trim()) return [];
		const options = Array.isArray(value.options)
			? value.options.flatMap((option) => {
					if (!option || typeof option !== "object") return [];
					const { label, description } = option as Record<string, unknown>;
					return typeof label === "string" && label
						? [{ label, description: typeof description === "string" ? description : "" }]
						: [];
				})
			: [];
		return [
			{
				id: typeof value.id === "string" && value.id ? value.id : question,
				header: typeof value.header === "string" ? value.header : "",
				question,
				options,
			},
		];
	});
}
