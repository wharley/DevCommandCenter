/**
 * Folds the raw `SessionEvent` stream into a flat list of "messages" that the
 * chat view can render in order. Each turn produces:
 *   - a `user` message from `turn_started.prompt`
 *   - one `assistant` message accumulating `turn_delta.content`
 *   - one collapsed `reasoning` block per `turn_reasoning_*` pair
 *   - one `tool` block per tool-call lifecycle
 *   - one `permission` block per pending permission request
 *
 * Designed to be incremental: pass the same `state` back in to apply later
 * events without re-walking history.
 */

export type RawSessionEvent = {
	eventId: string;
	sessionId: string;
	sequence: number;
	occurredAt: string;
	kind: SessionEventKind;
};

export type SessionEventKind = {
	type: string;
	turnId?: string;
	prompt?: string;
	content?: string;
	label?: string;
	reasoningId?: string;
	toolCallId?: string;
	toolName?: string;
	toolInput?: unknown;
	output?: unknown;
	error?: string;
	requestId?: string;
	question?: string;
	choices?: Array<{ id: string; label: string }>;
	[k: string]: unknown;
};

export type ChatMessage =
	| {
			kind: "user";
			turnId: string;
			text: string;
			at: string;
	  }
	| {
			kind: "assistant";
			turnId: string;
			text: string;
			at: string;
			completed: boolean;
			aborted: boolean;
	  }
	| {
			kind: "reasoning";
			turnId: string;
			id: string;
			label: string;
			at: string;
			completed: boolean;
	  }
	| {
			kind: "tool";
			turnId: string;
			toolCallId: string;
			toolName: string;
			input: unknown;
			output: unknown;
			error: string | null;
			status: "running" | "completed" | "failed";
			at: string;
	  }
	| {
			kind: "permission";
			turnId: string;
			requestId: string;
			question: string;
			choices: Array<{ id: string; label: string }>;
			at: string;
			resolved: boolean;
	  }
	| {
			kind: "system";
			id: string;
			text: string;
			at: string;
	  };

export type ThreadState = {
	/** Highest sequence number already applied. */
	cursor: number;
	/** Messages in the order they will render. Mutated by `applyEvents`. */
	messages: ChatMessage[];
	/** Lookup tables so we can mutate in-place when later events arrive. */
	byTurn: Map<string, { user?: number; assistant?: number }>;
	byTool: Map<string, number>;
	byReasoning: Map<string, number>;
	byPermission: Map<string, number>;
};

export function createThreadState(): ThreadState {
	return {
		cursor: 0,
		messages: [],
		byTurn: new Map(),
		byTool: new Map(),
		byReasoning: new Map(),
		byPermission: new Map(),
	};
}

export function applyEvents(state: ThreadState, events: RawSessionEvent[]): ThreadState {
	// Sort defensively; SSE batches sometimes arrive slightly out of order.
	const ordered = [...events]
		.filter((e) => e.sequence > state.cursor)
		.sort((a, b) => a.sequence - b.sequence);

	const next: ThreadState = {
		cursor: state.cursor,
		messages: state.messages.slice(),
		byTurn: new Map(state.byTurn),
		byTool: new Map(state.byTool),
		byReasoning: new Map(state.byReasoning),
		byPermission: new Map(state.byPermission),
	};

	for (const event of ordered) {
		applyOne(next, event);
		next.cursor = event.sequence;
	}
	return next;
}

function applyOne(state: ThreadState, event: RawSessionEvent) {
	const kind = event.kind ?? { type: "" };
	const turnId = kind.turnId ?? "";

	switch (kind.type) {
		case "session_started":
		case "session_resumed":
			pushSystem(state, event.eventId, "Sessão iniciada", event.occurredAt);
			break;

		case "session_completed":
			pushSystem(state, event.eventId, "Sessão finalizada", event.occurredAt);
			break;

		case "session_aborted":
			pushSystem(state, event.eventId, "Sessão abortada", event.occurredAt);
			break;

		case "turn_started": {
			const text = typeof kind.prompt === "string" ? kind.prompt : "";
			const refs = state.byTurn.get(turnId) ?? {};
			state.messages.push({
				kind: "user",
				turnId,
				text,
				at: event.occurredAt,
			});
			refs.user = state.messages.length - 1;
			state.byTurn.set(turnId, refs);
			break;
		}

		case "turn_delta": {
			const refs = state.byTurn.get(turnId) ?? {};
			if (refs.assistant === undefined) {
				state.messages.push({
					kind: "assistant",
					turnId,
					text: "",
					at: event.occurredAt,
					completed: false,
					aborted: false,
				});
				refs.assistant = state.messages.length - 1;
				state.byTurn.set(turnId, refs);
			}
			const msg = state.messages[refs.assistant!] as Extract<
				ChatMessage,
				{ kind: "assistant" }
			>;
			msg.text += typeof kind.content === "string" ? kind.content : "";
			break;
		}

		case "turn_completed": {
			const refs = state.byTurn.get(turnId);
			if (refs?.assistant !== undefined) {
				const msg = state.messages[refs.assistant] as Extract<
					ChatMessage,
					{ kind: "assistant" }
				>;
				msg.completed = true;
			}
			break;
		}

		case "turn_aborted": {
			const refs = state.byTurn.get(turnId);
			if (refs?.assistant !== undefined) {
				const msg = state.messages[refs.assistant] as Extract<
					ChatMessage,
					{ kind: "assistant" }
				>;
				msg.completed = true;
				msg.aborted = true;
			}
			break;
		}

		case "turn_reasoning_started": {
			const id =
				typeof kind.reasoningId === "string" ? kind.reasoningId : event.eventId;
			state.messages.push({
				kind: "reasoning",
				turnId,
				id,
				label: typeof kind.label === "string" ? kind.label : "Thinking",
				at: event.occurredAt,
				completed: false,
			});
			state.byReasoning.set(id, state.messages.length - 1);
			break;
		}

		case "turn_reasoning_completed": {
			const id = typeof kind.reasoningId === "string" ? kind.reasoningId : "";
			const idx = state.byReasoning.get(id);
			if (idx !== undefined) {
				(
					state.messages[idx] as Extract<ChatMessage, { kind: "reasoning" }>
				).completed = true;
			}
			break;
		}

		case "turn_tool_call_started": {
			const id =
				typeof kind.toolCallId === "string" ? kind.toolCallId : event.eventId;
			state.messages.push({
				kind: "tool",
				turnId,
				toolCallId: id,
				toolName:
					typeof kind.toolName === "string" ? kind.toolName : "tool",
				input: kind.toolInput,
				output: null,
				error: null,
				status: "running",
				at: event.occurredAt,
			});
			state.byTool.set(id, state.messages.length - 1);
			break;
		}

		case "turn_tool_call_completed": {
			const id =
				typeof kind.toolCallId === "string" ? kind.toolCallId : "";
			const idx = state.byTool.get(id);
			if (idx !== undefined) {
				const msg = state.messages[idx] as Extract<ChatMessage, { kind: "tool" }>;
				msg.status = "completed";
				msg.output = kind.output ?? null;
			}
			break;
		}

		case "turn_tool_call_failed": {
			const id =
				typeof kind.toolCallId === "string" ? kind.toolCallId : "";
			const idx = state.byTool.get(id);
			if (idx !== undefined) {
				const msg = state.messages[idx] as Extract<ChatMessage, { kind: "tool" }>;
				msg.status = "failed";
				msg.error =
					typeof kind.error === "string" ? kind.error : "Tool call failed.";
			}
			break;
		}

		case "turn_permission_requested": {
			const id =
				typeof kind.requestId === "string" ? kind.requestId : event.eventId;
			const choices = Array.isArray(kind.choices)
				? (kind.choices as Array<{ id: string; label: string }>)
				: [
						{ id: "allow", label: "Permitir" },
						{ id: "deny", label: "Negar" },
					];
			state.messages.push({
				kind: "permission",
				turnId,
				requestId: id,
				question:
					typeof kind.question === "string"
						? kind.question
						: "Pedido de permissão",
				choices,
				at: event.occurredAt,
				resolved: false,
			});
			state.byPermission.set(id, state.messages.length - 1);
			break;
		}

		case "turn_permission_resolved": {
			const id = typeof kind.requestId === "string" ? kind.requestId : "";
			const idx = state.byPermission.get(id);
			if (idx !== undefined) {
				(
					state.messages[idx] as Extract<ChatMessage, { kind: "permission" }>
				).resolved = true;
			}
			break;
		}

		default:
			// Unknown event types — silently skip. We do NOT log to keep the
			// console quiet for forward compatibility (server may emit new
			// kinds without breaking old clients).
			break;
	}
}

function pushSystem(
	state: ThreadState,
	id: string,
	text: string,
	at: string,
) {
	state.messages.push({ kind: "system", id, text, at });
}
