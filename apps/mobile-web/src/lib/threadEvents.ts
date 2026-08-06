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
	messageId?: string;
	phase?: "commentary" | "final_answer" | "unknown";
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

type CorePayload = Record<string, unknown>;

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

export function applyIncomingEvent(
	state: ThreadState,
	payload: unknown,
): ThreadState {
	if (isRawSessionEvent(payload)) {
		return applyEvents(state, [payload]);
	}

	const event = coreEventToRawSessionEvent(payload, state.cursor + 1);
	return event ? applyEvents(state, [event]) : state;
}

export function incomingEventSessionId(payload: unknown): string | null {
	if (isRawSessionEvent(payload)) {
		return payload.sessionId;
	}
	if (!payload || typeof payload !== "object") return null;
	const core = payload as CorePayload;
	const rawBody = Object.values(core).find(
		(value) => !!value && typeof value === "object",
	);
	if (!rawBody || typeof rawBody !== "object") return null;
	return stringField(rawBody as CorePayload, "session_id") ?? null;
}

export function incomingEventKindType(payload: unknown): string | null {
	if (isRawSessionEvent(payload)) {
		return payload.kind.type;
	}
	return coreEventToRawSessionEvent(payload, 0)?.kind.type ?? null;
}

function isRawSessionEvent(value: unknown): value is RawSessionEvent {
	if (!value || typeof value !== "object") return false;
	const candidate = value as Partial<RawSessionEvent>;
	return (
		typeof candidate.eventId === "string" &&
		typeof candidate.sessionId === "string" &&
		typeof candidate.sequence === "number" &&
		typeof candidate.occurredAt === "string" &&
		!!candidate.kind &&
		typeof candidate.kind === "object" &&
		typeof candidate.kind.type === "string"
	);
}

function coreEventToRawSessionEvent(
	payload: unknown,
	sequence: number,
): RawSessionEvent | null {
	if (!payload || typeof payload !== "object") return null;
	const core = payload as CorePayload;
	const [name, rawBody] =
		Object.entries(core).find(([, value]) => !!value && typeof value === "object") ?? [];
	if (!name || !rawBody || typeof rawBody !== "object") return null;

	const body = rawBody as CorePayload;
	const sessionId = stringField(body, "session_id");
	if (!sessionId) return null;

	const at = new Date().toISOString();
	const base = {
		eventId: `core:${name}:${sequence}`,
		sessionId,
		sequence,
		occurredAt: at,
	};

	switch (name) {
		case "sessionStarted":
			return {
				...base,
				kind: {
					type: "session_started",
					workspaceId: stringField(body, "workspace_id"),
					projectId: stringField(body, "project_id"),
					providerId: stringField(body, "provider_id"),
					model: stringField(body, "model"),
				},
			};
		case "sessionCompleted":
			return { ...base, kind: { type: "session_completed" } };
		case "sessionAborted":
			return {
				...base,
				kind: { type: "session_aborted", reason: stringField(body, "reason") },
			};
		case "sessionResumed":
			return { ...base, kind: { type: "session_resumed" } };
		case "sessionTurnStarted":
			return {
				...base,
				kind: {
					type: "turn_started",
					turnId: stringField(body, "turn_id"),
					prompt: stringField(body, "prompt") ?? "",
					planMode: booleanField(body, "plan_mode"),
				},
			};
		case "sessionTurnDelta":
			return {
				...base,
				kind: {
					type: "turn_delta",
					turnId: stringField(body, "turn_id"),
					content: stringField(body, "content") ?? "",
				},
			};
		case "sessionTurnAssistantMessageStarted":
			return {
				...base,
				kind: {
					type: "turn_assistant_message_started",
					turnId: stringField(body, "turn_id"),
					messageId: stringField(body, "message_id"),
					phase: assistantPhaseField(body, "phase"),
				},
			};
		case "sessionTurnAssistantMessageDelta":
			return {
				...base,
				kind: {
					type: "turn_assistant_message_delta",
					turnId: stringField(body, "turn_id"),
					messageId: stringField(body, "message_id"),
					content: stringField(body, "content") ?? "",
				},
			};
		case "sessionTurnAssistantMessageCompleted":
			return {
				...base,
				kind: {
					type: "turn_assistant_message_completed",
					turnId: stringField(body, "turn_id"),
					messageId: stringField(body, "message_id"),
					phase: assistantPhaseField(body, "phase"),
					content: stringField(body, "content"),
				},
			};
		case "sessionTurnReasoningStarted":
			return {
				...base,
				kind: {
					type: "turn_reasoning_started",
					turnId: stringField(body, "turn_id"),
					reasoningId: stringField(body, "reasoning_id"),
					label: stringField(body, "label") ?? "Thinking",
				},
			};
		case "sessionTurnReasoningCompleted":
			return {
				...base,
				kind: {
					type: "turn_reasoning_completed",
					turnId: stringField(body, "turn_id"),
					reasoningId: stringField(body, "reasoning_id"),
				},
			};
		case "sessionTurnToolCallStarted":
			return {
				...base,
				kind: {
					type: "turn_tool_call_started",
					turnId: stringField(body, "turn_id"),
					toolCallId: stringField(body, "tool_call_id"),
					toolName: stringField(body, "action") ?? stringField(body, "tool_name") ?? "tool",
					toolInput: {
						command: stringField(body, "command"),
						file: stringField(body, "file"),
					},
				},
			};
		case "sessionTurnToolCallCompleted":
			return {
				...base,
				kind: {
					type: "turn_tool_call_completed",
					turnId: stringField(body, "turn_id"),
					toolCallId: stringField(body, "tool_call_id"),
				},
			};
		case "sessionTurnToolCallFailed":
			return {
				...base,
				kind: {
					type: "turn_tool_call_failed",
					turnId: stringField(body, "turn_id"),
					toolCallId: stringField(body, "tool_call_id"),
					error: stringField(body, "reason") ?? "Tool call failed.",
				},
			};
		case "sessionTurnPermissionRequested":
			return {
				...base,
				kind: {
					type: "turn_permission_requested",
					turnId: stringField(body, "turn_id"),
					requestId: stringField(body, "request_id"),
					question: permissionQuestion(body),
					choices: [
						{ id: "allow", label: "Permitir" },
						{ id: "deny", label: "Negar" },
					],
				},
			};
		case "sessionTurnPermissionResolved":
			return {
				...base,
				kind: {
					type: "turn_permission_resolved",
					turnId: stringField(body, "turn_id"),
					requestId: stringField(body, "request_id"),
					behavior: stringField(body, "behavior"),
				},
			};
		case "sessionTurnCompleted":
			return {
				...base,
				kind: { type: "turn_completed", turnId: stringField(body, "turn_id") },
			};
		case "sessionTurnAborted":
			return {
				...base,
				kind: {
					type: "turn_aborted",
					turnId: stringField(body, "turn_id"),
					reason: stringField(body, "reason"),
				},
			};
		default:
			return null;
	}
}

function stringField(source: CorePayload, key: string): string | undefined {
	const value = source[key];
	return typeof value === "string" ? value : undefined;
}

function assistantPhaseField(
	source: CorePayload,
	key: string,
): "commentary" | "final_answer" | "unknown" {
	const phase = stringField(source, key);
	return phase === "commentary" || phase === "final_answer" ? phase : "unknown";
}

function booleanField(source: CorePayload, key: string): boolean | undefined {
	const value = source[key];
	return typeof value === "boolean" ? value : undefined;
}

function permissionQuestion(body: CorePayload): string {
	const text = [
		stringField(body, "title"),
		stringField(body, "description"),
		stringField(body, "command"),
		stringField(body, "file"),
	]
		.filter((value): value is string => !!value && value.trim().length > 0)
		.join("\n");
	return text || "Pedido de permissão";
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
			if (refs.user !== undefined) {
				break;
			}
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

		case "turn_delta":
		case "turn_assistant_message_delta": {
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

		case "turn_assistant_message_completed": {
			const refs = state.byTurn.get(turnId);
			if (
				refs?.assistant !== undefined &&
				kind.phase === "final_answer" &&
				typeof kind.content === "string"
			) {
				const msg = state.messages[refs.assistant] as Extract<
					ChatMessage,
					{ kind: "assistant" }
				>;
				msg.text = kind.content;
			}
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
			if (state.byReasoning.has(id)) {
				break;
			}
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
			if (state.byTool.has(id)) {
				break;
			}
			state.messages.push({
				kind: "tool",
				turnId,
				toolCallId: id,
				toolName:
					typeof kind.toolName === "string"
						? kind.toolName
						: typeof kind.action === "string"
							? kind.action
							: "tool",
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
					typeof kind.error === "string"
						? kind.error
						: typeof kind.reason === "string"
							? kind.reason
							: "Tool call failed.";
			}
			break;
		}

		case "turn_permission_requested": {
			const id =
				typeof kind.requestId === "string" ? kind.requestId : event.eventId;
			if (state.byPermission.has(id)) {
				break;
			}
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
						: permissionQuestion(kind),
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
