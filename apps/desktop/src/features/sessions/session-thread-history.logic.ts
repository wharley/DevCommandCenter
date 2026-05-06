import type {
	CoreEvent,
	ProviderUserInputAnswer,
	ProviderUserInputQuestion,
	SessionEventRecord,
} from "@dcc/contracts";
import { parsePlanContent, type ParsedPlanContent } from "@/features/panel/plan-content";

export type SessionMessageStatus = {
	type: "incomplete";
	reason?: string;
};

export type WorkspaceMessageRole = "user" | "assistant" | "system";

export type WorkspaceMessageAnnotation =
	| {
			type: "reasoning";
			id: string;
			label?: string;
			content: string;
			streaming?: boolean;
			createdAt?: string;
	  }
	| {
			type: "tool-call";
			id: string;
			action: string;
			command?: string;
			file?: string;
			content: string;
			streaming?: boolean;
			createdAt?: string;
			status?: {
				type: "failed";
				reason?: string;
			};
	  }
	| {
			type: "user-input";
			id: string;
			questions: ProviderUserInputQuestion[];
			answers: ProviderUserInputAnswer[];
			streaming?: boolean;
			createdAt?: string;
	  }
	| {
			type: "approval";
			id: string;
			toolName: string;
			title?: string;
			description?: string;
			command?: string;
			file?: string;
			behavior?: string;
			streaming?: boolean;
			createdAt?: string;
	  };

export type WorkspaceMessage = {
	id: string;
	role: WorkspaceMessageRole;
	content: string;
	label: string;
	streaming?: boolean;
	createdAt?: string;
	status?: SessionMessageStatus;
	annotations?: WorkspaceMessageAnnotation[];
	plan?: ParsedPlanContent | null;
	planMode?: boolean;
};

type TimelineEvent = {
	event: CoreEvent;
	occurredAt?: string;
	signature: string;
};

function recordToCoreEvent(record: SessionEventRecord): CoreEvent | null {
	switch (record.kind.type) {
		case "session_started":
			return {
				sessionStarted: {
					session_id: record.sessionId,
					workspace_id: record.kind.workspaceId,
					project_id: record.kind.projectId,
					provider_id: record.kind.providerId,
					model: record.kind.model,
				},
			};
		case "turn_started":
			return {
				sessionTurnStarted: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					prompt: record.kind.prompt,
					plan_mode: record.kind.planMode ?? null,
				},
			};
		case "turn_delta":
			return {
				sessionTurnDelta: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					content: record.kind.content,
				},
			};
		case "turn_reasoning_started":
			return {
				sessionTurnReasoningStarted: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					reasoning_id: record.kind.reasoningId,
					label: record.kind.label,
				},
			};
		case "turn_reasoning_delta":
			return {
				sessionTurnReasoningDelta: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					reasoning_id: record.kind.reasoningId,
					content: record.kind.content,
				},
			};
		case "turn_reasoning_completed":
			return {
				sessionTurnReasoningCompleted: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					reasoning_id: record.kind.reasoningId,
				},
			};
		case "turn_tool_call_started":
			return {
				sessionTurnToolCallStarted: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					tool_call_id: record.kind.toolCallId,
					action: record.kind.action,
					command: record.kind.command,
					file: record.kind.file,
				},
			};
		case "turn_tool_call_delta":
			return {
				sessionTurnToolCallDelta: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					tool_call_id: record.kind.toolCallId,
					content: record.kind.content,
				},
			};
		case "turn_tool_call_completed":
			return {
				sessionTurnToolCallCompleted: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					tool_call_id: record.kind.toolCallId,
				},
			};
		case "turn_tool_call_failed":
			return {
				sessionTurnToolCallFailed: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					tool_call_id: record.kind.toolCallId,
					reason: record.kind.reason,
				},
			};
		case "turn_user_input_requested":
			return {
				sessionTurnUserInputRequested: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					request_id: record.kind.requestId,
					questions: record.kind.questions,
				},
			};
		case "turn_user_input_resolved":
			return {
				sessionTurnUserInputResolved: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					request_id: record.kind.requestId,
					answers: record.kind.answers,
				},
			};
		case "turn_permission_requested":
			return {
				sessionTurnPermissionRequested: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					request_id: record.kind.requestId,
					tool_name: record.kind.toolName,
					title: record.kind.title,
					description: record.kind.description,
					command: record.kind.command,
					file: record.kind.file,
				},
			};
		case "turn_permission_resolved":
			return {
				sessionTurnPermissionResolved: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					request_id: record.kind.requestId,
					behavior: record.kind.behavior,
				},
			};
		case "turn_completed":
			return {
				sessionTurnCompleted: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
				},
			};
		case "turn_aborted":
			return {
				sessionTurnAborted: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					reason: record.kind.reason,
				},
			};
		case "checkpoint_created":
			return {
				sessionCheckpointCreated: {
					session_id: record.sessionId,
					checkpoint_id: record.kind.checkpointId,
					label: record.kind.label,
				},
			};
		case "session_completed":
			return {
				sessionCompleted: {
					session_id: record.sessionId,
				},
			};
		case "session_aborted":
			return {
				sessionAborted: {
					session_id: record.sessionId,
					reason: record.kind.reason,
				},
			};
		case "session_resumed":
			return {
				sessionResumed: {
					session_id: record.sessionId,
				},
			};
		default:
			return null;
	}
}

function getEventSessionId(event: CoreEvent): string | null {
	if ("sessionStarted" in event && event.sessionStarted) {
		return event.sessionStarted.session_id;
	}
	if ("sessionCompleted" in event && event.sessionCompleted) {
		return event.sessionCompleted.session_id;
	}
	if ("sessionAborted" in event && event.sessionAborted) {
		return event.sessionAborted.session_id;
	}
	if ("sessionResumed" in event && event.sessionResumed) {
		return event.sessionResumed.session_id;
	}
	if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
		return event.sessionTurnStarted.session_id;
	}
	if ("sessionTurnDelta" in event && event.sessionTurnDelta) {
		return event.sessionTurnDelta.session_id;
	}
	if ("sessionTurnReasoningStarted" in event && event.sessionTurnReasoningStarted) {
		return event.sessionTurnReasoningStarted.session_id;
	}
	if ("sessionTurnReasoningDelta" in event && event.sessionTurnReasoningDelta) {
		return event.sessionTurnReasoningDelta.session_id;
	}
	if ("sessionTurnReasoningCompleted" in event && event.sessionTurnReasoningCompleted) {
		return event.sessionTurnReasoningCompleted.session_id;
	}
	if ("sessionTurnToolCallStarted" in event && event.sessionTurnToolCallStarted) {
		return event.sessionTurnToolCallStarted.session_id;
	}
	if ("sessionTurnToolCallDelta" in event && event.sessionTurnToolCallDelta) {
		return event.sessionTurnToolCallDelta.session_id;
	}
	if ("sessionTurnToolCallCompleted" in event && event.sessionTurnToolCallCompleted) {
		return event.sessionTurnToolCallCompleted.session_id;
	}
	if ("sessionTurnToolCallFailed" in event && event.sessionTurnToolCallFailed) {
		return event.sessionTurnToolCallFailed.session_id;
	}
	if ("sessionTurnUserInputRequested" in event && event.sessionTurnUserInputRequested) {
		return event.sessionTurnUserInputRequested.session_id;
	}
	if ("sessionTurnUserInputResolved" in event && event.sessionTurnUserInputResolved) {
		return event.sessionTurnUserInputResolved.session_id;
	}
	if ("sessionTurnPermissionRequested" in event && event.sessionTurnPermissionRequested) {
		return event.sessionTurnPermissionRequested.session_id;
	}
	if ("sessionTurnPermissionResolved" in event && event.sessionTurnPermissionResolved) {
		return event.sessionTurnPermissionResolved.session_id;
	}
	if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
		return event.sessionTurnCompleted.session_id;
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return event.sessionTurnAborted.session_id;
	}
	if ("sessionCheckpointCreated" in event && event.sessionCheckpointCreated) {
		return event.sessionCheckpointCreated.session_id;
	}
	return null;
}

function eventLabel(event: CoreEvent): string {
	if ("sessionStarted" in event) return "session.started";
	if ("sessionCompleted" in event) return "session.completed";
	if ("sessionAborted" in event) return "session.aborted";
	if ("sessionResumed" in event) return "session.resumed";
	if ("sessionTurnStarted" in event) return "session.turn.started";
	if ("sessionTurnDelta" in event) return "session.turn.delta";
	if ("sessionTurnReasoningStarted" in event) return "session.turn.reasoning.started";
	if ("sessionTurnReasoningDelta" in event) return "session.turn.reasoning.delta";
	if ("sessionTurnReasoningCompleted" in event) return "session.turn.reasoning.completed";
	if ("sessionTurnToolCallStarted" in event) return "session.turn.tool-call.started";
	if ("sessionTurnToolCallDelta" in event) return "session.turn.tool-call.delta";
	if ("sessionTurnToolCallCompleted" in event) return "session.turn.tool-call.completed";
	if ("sessionTurnToolCallFailed" in event) return "session.turn.tool-call.failed";
	if ("sessionTurnUserInputRequested" in event) return "session.turn.user-input.requested";
	if ("sessionTurnUserInputResolved" in event) return "session.turn.user-input.resolved";
	if ("sessionTurnPermissionRequested" in event) return "session.turn.permission.requested";
	if ("sessionTurnPermissionResolved" in event) return "session.turn.permission.resolved";
	if ("sessionTurnCompleted" in event) return "session.turn.completed";
	if ("sessionTurnAborted" in event) return "session.turn.aborted";
	if ("sessionCheckpointCreated" in event) return "session.checkpoint.created";
	if ("workspacePrepared" in event) return "workspace.prepared";
	if ("workspaceReady" in event) return "workspace.ready";
	return "event";
}

function eventSummary(event: CoreEvent): string {
	if ("sessionStarted" in event && event.sessionStarted) {
		return `${event.sessionStarted.session_id} · ${event.sessionStarted.provider_id}`;
	}
	if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
		return event.sessionTurnStarted.prompt;
	}
	if ("sessionTurnDelta" in event && event.sessionTurnDelta) {
		return event.sessionTurnDelta.content;
	}
	if ("sessionTurnReasoningStarted" in event && event.sessionTurnReasoningStarted) {
		return event.sessionTurnReasoningStarted.label ?? event.sessionTurnReasoningStarted.reasoning_id;
	}
	if ("sessionTurnReasoningDelta" in event && event.sessionTurnReasoningDelta) {
		return event.sessionTurnReasoningDelta.content;
	}
	if ("sessionTurnReasoningCompleted" in event && event.sessionTurnReasoningCompleted) {
		return event.sessionTurnReasoningCompleted.reasoning_id;
	}
	if ("sessionTurnToolCallStarted" in event && event.sessionTurnToolCallStarted) {
		const command = event.sessionTurnToolCallStarted.command ?? "";
		const file = event.sessionTurnToolCallStarted.file ?? "";
		return `${event.sessionTurnToolCallStarted.action}${command ? ` · ${command}` : ""}${file ? ` · ${file}` : ""}`;
	}
	if ("sessionTurnToolCallDelta" in event && event.sessionTurnToolCallDelta) {
		return event.sessionTurnToolCallDelta.content;
	}
	if ("sessionTurnToolCallCompleted" in event && event.sessionTurnToolCallCompleted) {
		return event.sessionTurnToolCallCompleted.tool_call_id;
	}
	if ("sessionTurnToolCallFailed" in event && event.sessionTurnToolCallFailed) {
		return event.sessionTurnToolCallFailed.reason ?? "Tool call failed";
	}
	if ("sessionTurnUserInputRequested" in event && event.sessionTurnUserInputRequested) {
		return (
			event.sessionTurnUserInputRequested.questions[0]?.question ??
			"User input requested"
		);
	}
	if ("sessionTurnUserInputResolved" in event && event.sessionTurnUserInputResolved) {
		return event.sessionTurnUserInputResolved.answers
			.map((answer) => `${answer.question}: ${answer.answer}`)
			.join(" · ");
	}
	if ("sessionTurnPermissionRequested" in event && event.sessionTurnPermissionRequested) {
		const command = event.sessionTurnPermissionRequested.command ?? "";
		const file = event.sessionTurnPermissionRequested.file ?? "";
		return `${event.sessionTurnPermissionRequested.tool_name}${command ? ` · ${command}` : ""}${file ? ` · ${file}` : ""}`;
	}
	if ("sessionTurnPermissionResolved" in event && event.sessionTurnPermissionResolved) {
		return event.sessionTurnPermissionResolved.behavior;
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return event.sessionTurnAborted.reason ?? "Turn aborted";
	}
	if ("sessionCheckpointCreated" in event && event.sessionCheckpointCreated) {
		return event.sessionCheckpointCreated.label;
	}
	if ("workspacePrepared" in event && event.workspacePrepared) {
		return `${event.workspacePrepared.project_id} · ${event.workspacePrepared.worktree_path}`;
	}
	if ("workspaceReady" in event && event.workspaceReady) {
		return `${event.workspaceReady.project_id} · ${event.workspaceReady.worktree_path}`;
	}
	if ("sessionCompleted" in event && event.sessionCompleted) {
		return event.sessionCompleted.session_id;
	}
	if ("sessionAborted" in event && event.sessionAborted) {
		return event.sessionAborted.reason ?? "Session aborted";
	}
	if ("sessionResumed" in event && event.sessionResumed) {
		return event.sessionResumed.session_id;
	}
	return "No payload summary";
}

function eventSignature(event: CoreEvent): string {
	return JSON.stringify(event);
}

function pushHistoryEvent(
	events: TimelineEvent[],
	seen: Set<string>,
	record: SessionEventRecord,
) {
	const coreEvent = recordToCoreEvent(record);
	if (!coreEvent) {
		return;
	}

	const signature = eventSignature(coreEvent);
	if (seen.has(signature)) {
		return;
	}

	seen.add(signature);
	events.push({
		event: coreEvent,
		occurredAt: record.occurredAt,
		signature,
	});
}

function pushLiveEvent(
	events: TimelineEvent[],
	seen: Set<string>,
	event: CoreEvent,
) {
	const signature = eventSignature(event);
	if (seen.has(signature)) {
		return;
	}

	seen.add(signature);
	events.push({
		event,
		signature,
	});
}

export function mergeSessionThreadEvents(
	historyEvents: SessionEventRecord[],
	liveEvents: CoreEvent[],
	sessionId: string | null,
) {
	const merged: TimelineEvent[] = [];
	const seen = new Set<string>();

	for (const record of historyEvents) {
		if (sessionId !== null && record.sessionId !== sessionId) {
			continue;
		}
		pushHistoryEvent(merged, seen, record);
	}

	for (const event of liveEvents) {
		if (sessionId !== null && getEventSessionId(event) !== sessionId) {
			continue;
		}
		pushLiveEvent(merged, seen, event);
	}

	return merged;
}

function ensureAssistantMessage(
	messages: WorkspaceMessage[],
	assistantBuckets: Map<string, WorkspaceMessage>,
	sessionId: string,
	turnId: string,
	createdAt?: string,
) {
	let message = assistantBuckets.get(turnId);
	if (message) {
		return message;
	}

	message = {
		id: `assistant-${sessionId}-${turnId}`,
		role: "assistant",
		label: "Assistant",
		content: "",
		streaming: true,
		createdAt,
	};
	assistantBuckets.set(turnId, message);
	messages.push(message);
	return message;
}

function getOrCreateAnnotation(
	message: WorkspaceMessage,
	annotation: WorkspaceMessageAnnotation,
) {
	const annotations = message.annotations ?? (message.annotations = []);
	const existingIndex = annotations.findIndex(
		(item) => item.type === annotation.type && item.id === annotation.id,
	);
	if (existingIndex >= 0) {
		return annotations[existingIndex];
	}
	annotations.push(annotation);
	return annotation;
}

function isHiddenToolAction(action: string | undefined) {
	return action === "AskUserQuestion" || action === "ExitPlanMode";
}

export function projectWorkspaceMessages(
	historyEvents: SessionEventRecord[],
	liveEvents: CoreEvent[],
	sessionId: string | null = null,
	pendingPrompt: string | null = null,
): WorkspaceMessage[] {
	const messages: WorkspaceMessage[] = [];
	const assistantBuckets = new Map<string, WorkspaceMessage>();
	const completedTurns = new Set<string>();
	const abortedTurns = new Map<string, string>();
	const turnStartedAtByTurnId = new Map<string, string>();
	const filteredEvents = mergeSessionThreadEvents(historyEvents, liveEvents, sessionId);

	for (const timelineEvent of filteredEvents) {
		const event = timelineEvent.event;
		const occurredAt = timelineEvent.occurredAt;

		if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
			turnStartedAtByTurnId.set(
				event.sessionTurnStarted.turn_id,
				occurredAt ?? "",
			);
			messages.push({
				id: `user-${event.sessionTurnStarted.session_id}-${event.sessionTurnStarted.turn_id}`,
				role: "user",
				label: "User",
				content: event.sessionTurnStarted.prompt,
				createdAt: occurredAt,
				planMode: event.sessionTurnStarted.plan_mode === true,
			});
			continue;
		}

		if ("sessionTurnDelta" in event && event.sessionTurnDelta) {
			const key = event.sessionTurnDelta.turn_id;
			const message = ensureAssistantMessage(
				messages,
				assistantBuckets,
				event.sessionTurnDelta.session_id,
				key,
				turnStartedAtByTurnId.get(key) ?? occurredAt,
			);
			message.content = `${message.content}${event.sessionTurnDelta.content}`;
			message.streaming = !completedTurns.has(key) && !abortedTurns.has(key);
			const abortedReason = abortedTurns.get(key);
			if (abortedReason) {
				message.streaming = false;
				message.status = {
					type: "incomplete",
					reason: abortedReason,
				};
			}
			continue;
		}

		if ("sessionTurnReasoningStarted" in event && event.sessionTurnReasoningStarted) {
			const key = event.sessionTurnReasoningStarted.turn_id;
			const message = ensureAssistantMessage(
				messages,
				assistantBuckets,
				event.sessionTurnReasoningStarted.session_id,
				key,
				turnStartedAtByTurnId.get(key) ?? occurredAt,
			);
			getOrCreateAnnotation(message, {
				type: "reasoning",
				id: event.sessionTurnReasoningStarted.reasoning_id,
				label: event.sessionTurnReasoningStarted.label ?? "Thinking",
				content: "",
				streaming: true,
				createdAt: occurredAt,
			});
			continue;
		}

		if ("sessionTurnReasoningDelta" in event && event.sessionTurnReasoningDelta) {
			const key = event.sessionTurnReasoningDelta.turn_id;
			const message = ensureAssistantMessage(
				messages,
				assistantBuckets,
				event.sessionTurnReasoningDelta.session_id,
				key,
				turnStartedAtByTurnId.get(key) ?? occurredAt,
			);
			const annotation = getOrCreateAnnotation(message, {
				type: "reasoning",
				id: event.sessionTurnReasoningDelta.reasoning_id,
				label: "Thinking",
				content: "",
				streaming: true,
				createdAt: occurredAt,
			});
			if (annotation.type === "reasoning") {
				annotation.content = `${annotation.content}${event.sessionTurnReasoningDelta.content}`;
				annotation.streaming = true;
				annotation.createdAt ??= occurredAt;
			}
			continue;
		}

		if ("sessionTurnReasoningCompleted" in event && event.sessionTurnReasoningCompleted) {
			const key = event.sessionTurnReasoningCompleted.turn_id;
			const message = assistantBuckets.get(key);
			const annotation = message?.annotations?.find(
				(item) =>
					item.type === "reasoning" &&
					item.id === event.sessionTurnReasoningCompleted.reasoning_id,
			);
			if (annotation && annotation.type === "reasoning") {
				annotation.streaming = false;
			}
			continue;
		}

		if ("sessionTurnToolCallStarted" in event && event.sessionTurnToolCallStarted) {
			if (isHiddenToolAction(event.sessionTurnToolCallStarted.action)) {
				continue;
			}
			const key = event.sessionTurnToolCallStarted.turn_id;
			const message = ensureAssistantMessage(
				messages,
				assistantBuckets,
				event.sessionTurnToolCallStarted.session_id,
				key,
				turnStartedAtByTurnId.get(key) ?? occurredAt,
			);
			getOrCreateAnnotation(message, {
				type: "tool-call",
				id: event.sessionTurnToolCallStarted.tool_call_id,
				action: event.sessionTurnToolCallStarted.action,
				command: event.sessionTurnToolCallStarted.command ?? undefined,
				file: event.sessionTurnToolCallStarted.file ?? undefined,
				content: "",
				streaming: true,
				createdAt: occurredAt,
			});
			continue;
		}

		if ("sessionTurnToolCallDelta" in event && event.sessionTurnToolCallDelta) {
			const key = event.sessionTurnToolCallDelta.turn_id;
			const message = ensureAssistantMessage(
				messages,
				assistantBuckets,
				event.sessionTurnToolCallDelta.session_id,
				key,
				turnStartedAtByTurnId.get(key) ?? occurredAt,
			);
			const annotation = message.annotations?.find(
				(item) =>
					item.type === "tool-call" &&
					item.id === event.sessionTurnToolCallDelta.tool_call_id,
			);
			if (annotation && annotation.type === "tool-call") {
				annotation.content = `${annotation.content}${event.sessionTurnToolCallDelta.content}`;
				annotation.streaming = true;
				annotation.createdAt ??= occurredAt;
			}
			continue;
		}

		if ("sessionTurnToolCallCompleted" in event && event.sessionTurnToolCallCompleted) {
			const key = event.sessionTurnToolCallCompleted.turn_id;
			const message = assistantBuckets.get(key);
			const annotation = message?.annotations?.find(
				(item) =>
					item.type === "tool-call" &&
					item.id === event.sessionTurnToolCallCompleted.tool_call_id,
			);
			if (annotation && annotation.type === "tool-call") {
				annotation.streaming = false;
			}
			continue;
		}

		if ("sessionTurnToolCallFailed" in event && event.sessionTurnToolCallFailed) {
			const key = event.sessionTurnToolCallFailed.turn_id;
			const message = assistantBuckets.get(key);
			const annotation = message?.annotations?.find(
				(item) =>
					item.type === "tool-call" &&
					item.id === event.sessionTurnToolCallFailed.tool_call_id,
			);
			if (annotation && annotation.type === "tool-call") {
				annotation.streaming = false;
				annotation.status = {
					type: "failed",
					reason: event.sessionTurnToolCallFailed.reason ?? "Tool call failed",
				};
			}
			continue;
		}

		if ("sessionTurnUserInputRequested" in event && event.sessionTurnUserInputRequested) {
			const key = event.sessionTurnUserInputRequested.turn_id;
			const message = ensureAssistantMessage(
				messages,
				assistantBuckets,
				event.sessionTurnUserInputRequested.session_id,
				key,
				turnStartedAtByTurnId.get(key) ?? occurredAt,
			);
			getOrCreateAnnotation(message, {
				type: "user-input",
				id: event.sessionTurnUserInputRequested.request_id,
				questions: event.sessionTurnUserInputRequested.questions,
				answers: [],
				streaming: true,
				createdAt: occurredAt,
			});
			continue;
		}

		if ("sessionTurnUserInputResolved" in event && event.sessionTurnUserInputResolved) {
			const key = event.sessionTurnUserInputResolved.turn_id;
			const message = assistantBuckets.get(key);
			const annotation = message?.annotations?.find(
				(item) =>
					item.type === "user-input" &&
					item.id === event.sessionTurnUserInputResolved.request_id,
			);
			if (annotation && annotation.type === "user-input") {
				annotation.answers = event.sessionTurnUserInputResolved.answers;
				annotation.streaming = false;
				annotation.createdAt ??= occurredAt;
			}
			continue;
		}

		if ("sessionTurnPermissionRequested" in event && event.sessionTurnPermissionRequested) {
			const key = event.sessionTurnPermissionRequested.turn_id;
			const message = ensureAssistantMessage(
				messages,
				assistantBuckets,
				event.sessionTurnPermissionRequested.session_id,
				key,
				turnStartedAtByTurnId.get(key) ?? occurredAt,
			);
			getOrCreateAnnotation(message, {
				type: "approval",
				id: event.sessionTurnPermissionRequested.request_id,
				toolName: event.sessionTurnPermissionRequested.tool_name,
				title: event.sessionTurnPermissionRequested.title ?? undefined,
				description: event.sessionTurnPermissionRequested.description ?? undefined,
				command: event.sessionTurnPermissionRequested.command ?? undefined,
				file: event.sessionTurnPermissionRequested.file ?? undefined,
				streaming: true,
				createdAt: occurredAt,
			});
			continue;
		}

		if ("sessionTurnPermissionResolved" in event && event.sessionTurnPermissionResolved) {
			const key = event.sessionTurnPermissionResolved.turn_id;
			const message = assistantBuckets.get(key);
			const annotation = message?.annotations?.find(
				(item) =>
					item.type === "approval" &&
					item.id === event.sessionTurnPermissionResolved.request_id,
			);
			if (annotation && annotation.type === "approval") {
				annotation.behavior = event.sessionTurnPermissionResolved.behavior;
				annotation.streaming = false;
				annotation.createdAt ??= occurredAt;
			}
			continue;
		}

		if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
			const key = event.sessionTurnCompleted.turn_id;
			completedTurns.add(key);
			const existing = assistantBuckets.get(key);
			if (existing) {
				existing.streaming = false;
				for (const annotation of existing.annotations ?? []) {
					annotation.streaming = false;
				}
			}
			continue;
		}

		if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
			const key = event.sessionTurnAborted.turn_id;
			abortedTurns.set(key, event.sessionTurnAborted.reason ?? "Turn aborted");
			const existing = assistantBuckets.get(key);
			if (existing) {
				existing.streaming = false;
				existing.status = {
					type: "incomplete",
					reason: event.sessionTurnAborted.reason ?? "Turn aborted",
				};
				for (const annotation of existing.annotations ?? []) {
					annotation.streaming = false;
				}
			}
			continue;
		}

		if ("sessionStarted" in event) {
			messages.push({
				id: `${eventLabel(event)}-${messages.length}`,
				role: "system",
				label: eventLabel(event),
				content: eventSummary(event),
				createdAt: occurredAt,
			});
			continue;
		}

		if ("sessionCompleted" in event || "sessionAborted" in event || "sessionResumed" in event || "sessionCheckpointCreated" in event || "workspacePrepared" in event || "workspaceReady" in event) {
			messages.push({
				id: `${eventLabel(event)}-${messages.length}`,
				role: "system",
				label: eventLabel(event),
				content: eventSummary(event),
				createdAt: occurredAt,
			});
		}
	}

	const trimmedPendingPrompt = pendingPrompt?.trim();
	if (sessionId && trimmedPendingPrompt) {
		const hasPromptAlready = filteredEvents.some(
			(timelineEvent) =>
				"sessionTurnStarted" in timelineEvent.event &&
				timelineEvent.event.sessionTurnStarted?.session_id === sessionId &&
				timelineEvent.event.sessionTurnStarted.prompt.trim() === trimmedPendingPrompt,
		);
		if (!hasPromptAlready) {
			messages.push({
				id: `pending-user-${sessionId}`,
				role: "user",
				label: "User",
				content: trimmedPendingPrompt,
			});
		}
	}

	let lastUserPlanMode = false;
	for (const message of messages) {
		if (message.role === "user") {
			lastUserPlanMode = message.planMode === true;
			continue;
		}
		if (message.role !== "assistant") {
			continue;
		}
		if (!lastUserPlanMode) {
			continue;
		}
		const parsedPlan = parsePlanContent(message.content);
		if (parsedPlan.isPlanLike) {
			message.plan = parsedPlan;
		}
	}

	return messages;
}
