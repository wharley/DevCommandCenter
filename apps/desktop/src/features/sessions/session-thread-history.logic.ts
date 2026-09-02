import type {
	AssistantMessagePhase,
	CoreEvent,
	ProviderUserInputAnswer,
	ProviderUserInputQuestion,
	SessionEventRecord,
	TurnEvidenceSummary,
} from "@dcc/contracts";
import { parsePlanContent, type ParsedPlanContent } from "@/features/panel/plan-content";

export type SessionMessageStatus = {
	type: "incomplete";
	reason?: string;
};

export type WorkspaceMessageRole = "user" | "assistant" | "system";

export type WorkspaceMessageAnnotation =
	| {
			type: "commentary";
			id: string;
			content: string;
			streaming?: boolean;
			createdAt?: string;
	  }
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
	  }
	| {
			type: "native-subagent";
			id: string;
			agentId?: string;
			agentThreadId?: string;
			path?: string;
			name?: string;
			role?: string;
			model?: string;
			requestedModel?: string;
			status: "running" | "completed" | "failed";
			streaming?: boolean;
			createdAt?: string;
	  };

export type WorkspaceMessageDelegation = {
	id: string;
	phase: "requested" | "running" | "completed" | "failed" | "cancelled";
	childSessionId?: string | null;
	summary?: string | null;
	reason?: string | null;
};

export type WorkspaceMessage = {
	id: string;
	role: WorkspaceMessageRole;
	turnId?: string;
	assistantPhase?: AssistantMessagePhase;
	content: string;
	label: string;
	model?: string | null;
	streaming?: boolean;
	createdAt?: string;
	status?: SessionMessageStatus;
	annotations?: WorkspaceMessageAnnotation[];
	plan?: ParsedPlanContent | null;
	planMode?: boolean;
	/** Metadata-only evidence linkage recorded with the turn (no bodies). */
	evidence?: TurnEvidenceSummary;
	/** Explicit retry linkage: the aborted turn this user turn re-ran. */
	retryOfTurnId?: string;
	action?: {
		type: "open-session";
		sessionId: string;
		label: string;
	};
	delegation?: WorkspaceMessageDelegation;
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
					model: record.kind.model ?? null,
					...(record.kind.evidence ? { evidence: record.kind.evidence } : {}),
					...(record.kind.retryOfTurnId
						? { retry_of_turn_id: record.kind.retryOfTurnId }
						: {}),
				},
			};
		case "turn_steered":
			return {
				sessionTurnSteered: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					prompt: record.kind.prompt,
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
		case "turn_assistant_message_started":
			return {
				sessionTurnAssistantMessageStarted: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					message_id: record.kind.messageId,
					phase: record.kind.phase,
				},
			};
		case "turn_assistant_message_delta":
			return {
				sessionTurnAssistantMessageDelta: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					message_id: record.kind.messageId,
					content: record.kind.content,
				},
			};
		case "turn_assistant_message_completed":
			return {
				sessionTurnAssistantMessageCompleted: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					message_id: record.kind.messageId,
					phase: record.kind.phase,
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
		case "turn_native_subagent_activity":
			return {
				sessionTurnNativeSubagentActivity: {
					session_id: record.sessionId,
					turn_id: record.kind.turnId,
					id: record.kind.id,
					agent_id: record.kind.agentId,
					agent_thread_id: record.kind.agentThreadId,
					path: record.kind.path ?? null,
					name: record.kind.name,
					role: record.kind.role,
					model: record.kind.model,
					status: record.kind.status,
				},
			};
		case "turn_native_subagent_model_requested":
			return { sessionTurnNativeSubagentModelRequested: { session_id: record.sessionId, turn_id: record.kind.turnId, correlation_id: record.kind.correlationId, model: record.kind.model } };
		case "turn_native_subagent_model_confirmed":
			return { sessionTurnNativeSubagentModelConfirmed: { session_id: record.sessionId, turn_id: record.kind.turnId, correlation_id: record.kind.correlationId, model: record.kind.model } };
		case "turn_model_effective":
			return { sessionTurnModelEffective: { session_id: record.sessionId, turn_id: record.kind.turnId, model: record.kind.model } };
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
		case "plan_approved":
			return {
				sessionPlanApproved: {
					session_id: record.sessionId,
					plan_message_id: record.kind.planMessageId,
					plan_version: record.kind.planVersion,
					plan_hash: record.kind.planHash,
				},
			};
		case "plan_handed_off":
			return {
				sessionPlanHandedOff: {
					session_id: record.sessionId,
					plan_message_id: record.kind.planMessageId,
					plan_version: record.kind.planVersion,
					plan_hash: record.kind.planHash,
					action: record.kind.action,
					target_session_id: record.kind.targetSessionId,
				},
			};
		case "delegation_requested":
			return {
				sessionDelegationRequested: {
					session_id: record.sessionId,
					delegation_id: record.kind.delegationId,
				},
			};
		case "delegation_started":
			return {
				sessionDelegationStarted: {
					session_id: record.sessionId,
					delegation_id: record.kind.delegationId,
					child_session_id: record.kind.childSessionId,
				},
			};
		case "delegation_delta":
			return {
				sessionDelegationDelta: {
					session_id: record.sessionId,
					delegation_id: record.kind.delegationId,
					content: record.kind.content,
				},
			};
		case "delegation_completed":
			return {
				sessionDelegationCompleted: {
					session_id: record.sessionId,
					delegation_id: record.kind.delegationId,
					summary: record.kind.summary,
				},
			};
		case "delegation_failed":
			return {
				sessionDelegationFailed: {
					session_id: record.sessionId,
					delegation_id: record.kind.delegationId,
					reason: record.kind.reason,
				},
			};
		case "delegation_cancelled":
			return {
				sessionDelegationCancelled: {
					session_id: record.sessionId,
					delegation_id: record.kind.delegationId,
					reason: record.kind.reason,
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
		case "objective_paused":
			return {
				sessionObjectivePaused: {
					session_id: record.sessionId,
					reason: record.kind.reason,
					consecutive_failures: record.kind.consecutiveFailures,
					turns_used: record.kind.turnsUsed,
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
	if ("sessionObjectivePaused" in event && event.sessionObjectivePaused) {
		return event.sessionObjectivePaused.session_id;
	}
	if (
		"sessionMcpRuntimeStatusChanged" in event &&
		event.sessionMcpRuntimeStatusChanged
	) {
		return event.sessionMcpRuntimeStatusChanged.session_id;
	}
	if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
		return event.sessionTurnStarted.session_id;
	}
	if ("sessionTurnSteered" in event && event.sessionTurnSteered) {
		return event.sessionTurnSteered.session_id;
	}
	if ("sessionTurnDelta" in event && event.sessionTurnDelta) {
		return event.sessionTurnDelta.session_id;
	}
	if (
		"sessionTurnAssistantMessageStarted" in event &&
		event.sessionTurnAssistantMessageStarted
	) {
		return event.sessionTurnAssistantMessageStarted.session_id;
	}
	if (
		"sessionTurnAssistantMessageDelta" in event &&
		event.sessionTurnAssistantMessageDelta
	) {
		return event.sessionTurnAssistantMessageDelta.session_id;
	}
	if (
		"sessionTurnAssistantMessageCompleted" in event &&
		event.sessionTurnAssistantMessageCompleted
	) {
		return event.sessionTurnAssistantMessageCompleted.session_id;
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
	if (
		"sessionTurnNativeSubagentActivity" in event &&
		event.sessionTurnNativeSubagentActivity
	) {
		return event.sessionTurnNativeSubagentActivity.session_id;
	}
	if (
		"sessionTurnNativeSubagentModelRequested" in event &&
		event.sessionTurnNativeSubagentModelRequested
	) {
		return event.sessionTurnNativeSubagentModelRequested.session_id;
	}
	if (
		"sessionTurnNativeSubagentModelConfirmed" in event &&
		event.sessionTurnNativeSubagentModelConfirmed
	) {
		return event.sessionTurnNativeSubagentModelConfirmed.session_id;
	}
	if ("sessionTurnModelEffective" in event && event.sessionTurnModelEffective) {
		return event.sessionTurnModelEffective.session_id;
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
	if ("sessionPlanApproved" in event && event.sessionPlanApproved) {
		return event.sessionPlanApproved.session_id;
	}
	if ("sessionPlanHandedOff" in event && event.sessionPlanHandedOff) {
		return event.sessionPlanHandedOff.session_id;
	}
	if ("sessionDelegationRequested" in event && event.sessionDelegationRequested) {
		return event.sessionDelegationRequested.session_id;
	}
	if ("sessionDelegationStarted" in event && event.sessionDelegationStarted) {
		return event.sessionDelegationStarted.session_id;
	}
	if ("sessionDelegationDelta" in event && event.sessionDelegationDelta) {
		return event.sessionDelegationDelta.session_id;
	}
	if ("sessionDelegationCompleted" in event && event.sessionDelegationCompleted) {
		return event.sessionDelegationCompleted.session_id;
	}
	if ("sessionDelegationFailed" in event && event.sessionDelegationFailed) {
		return event.sessionDelegationFailed.session_id;
	}
	if ("sessionDelegationCancelled" in event && event.sessionDelegationCancelled) {
		return event.sessionDelegationCancelled.session_id;
	}
	return null;
}

function eventLabel(event: CoreEvent): string {
	if ("sessionStarted" in event) return "session.started";
	if ("sessionCompleted" in event) return "session.completed";
	if ("sessionAborted" in event) return "session.aborted";
	if ("sessionResumed" in event) return "session.resumed";
	if ("sessionObjectivePaused" in event) return "session.objective.paused";
	if ("sessionMcpRuntimeStatusChanged" in event) return "session.mcp.runtime-status";
	if ("sessionTurnStarted" in event) return "session.turn.started";
	if ("sessionTurnSteered" in event) return "session.turn.steered";
	if ("sessionTurnDelta" in event) return "session.turn.delta";
	if ("sessionTurnAssistantMessageStarted" in event)
		return "session.turn.assistant-message.started";
	if ("sessionTurnAssistantMessageDelta" in event)
		return "session.turn.assistant-message.delta";
	if ("sessionTurnAssistantMessageCompleted" in event)
		return "session.turn.assistant-message.completed";
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
	if ("sessionTurnNativeSubagentActivity" in event)
		return "session.turn.native-subagent.activity";
	if ("sessionTurnCompleted" in event) return "session.turn.completed";
	if ("sessionTurnAborted" in event) return "session.turn.aborted";
	if ("sessionCheckpointCreated" in event) return "session.checkpoint.created";
	if ("sessionPlanApproved" in event) return "session.plan.approved";
	if ("sessionPlanHandedOff" in event) return "session.plan.handed-off";
	if ("sessionDelegationRequested" in event) return "delegation.requested";
	if ("sessionDelegationStarted" in event) return "delegation.running";
	if ("sessionDelegationDelta" in event) return "delegation.update";
	if ("sessionDelegationCompleted" in event) return "delegation.completed";
	if ("sessionDelegationFailed" in event) return "delegation.failed";
	if ("sessionDelegationCancelled" in event) return "delegation.cancelled";
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
	if ("sessionTurnSteered" in event && event.sessionTurnSteered) {
		return event.sessionTurnSteered.prompt;
	}
	if ("sessionTurnDelta" in event && event.sessionTurnDelta) {
		return event.sessionTurnDelta.content;
	}
	if (
		"sessionTurnAssistantMessageStarted" in event &&
		event.sessionTurnAssistantMessageStarted
	) {
		return event.sessionTurnAssistantMessageStarted.phase;
	}
	if (
		"sessionTurnAssistantMessageDelta" in event &&
		event.sessionTurnAssistantMessageDelta
	) {
		return event.sessionTurnAssistantMessageDelta.content;
	}
	if (
		"sessionTurnAssistantMessageCompleted" in event &&
		event.sessionTurnAssistantMessageCompleted
	) {
		return (
			event.sessionTurnAssistantMessageCompleted.content ??
			event.sessionTurnAssistantMessageCompleted.phase
		);
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
	if (
		"sessionTurnNativeSubagentActivity" in event &&
		event.sessionTurnNativeSubagentActivity
	) {
		const item = event.sessionTurnNativeSubagentActivity;
		return [item.name ?? item.role ?? item.agent_id ?? item.agent_thread_id, item.model]
			.filter(Boolean)
			.join(" · ");
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return event.sessionTurnAborted.reason ?? "Turn aborted";
	}
	if ("sessionCheckpointCreated" in event && event.sessionCheckpointCreated) {
		return event.sessionCheckpointCreated.label;
	}
	if ("sessionPlanApproved" in event && event.sessionPlanApproved) {
		return `Plan version ${event.sessionPlanApproved.plan_version} approved`;
	}
	if ("sessionPlanHandedOff" in event && event.sessionPlanHandedOff) {
		return `Plan version ${event.sessionPlanHandedOff.plan_version} handed off via ${event.sessionPlanHandedOff.action}`;
	}
	if ("sessionDelegationRequested" in event && event.sessionDelegationRequested) {
		return `Delegation requested · ${event.sessionDelegationRequested.delegation_id}`;
	}
	if ("sessionDelegationStarted" in event && event.sessionDelegationStarted) {
		return event.sessionDelegationStarted.child_session_id
			? `Child session ${event.sessionDelegationStarted.child_session_id} is running.`
			: "Delegation is running.";
	}
	if ("sessionDelegationDelta" in event && event.sessionDelegationDelta) {
		return event.sessionDelegationDelta.content;
	}
	if ("sessionDelegationCompleted" in event && event.sessionDelegationCompleted) {
		return event.sessionDelegationCompleted.summary ?? "Delegation completed.";
	}
	if ("sessionDelegationFailed" in event && event.sessionDelegationFailed) {
		return event.sessionDelegationFailed.reason ?? "Delegation failed.";
	}
	if ("sessionDelegationCancelled" in event && event.sessionDelegationCancelled) {
		return event.sessionDelegationCancelled.reason ?? "Delegation cancelled.";
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
	if ("sessionObjectivePaused" in event && event.sessionObjectivePaused) {
		const paused = event.sessionObjectivePaused;
		const why =
			paused.reason === "consecutive_failures"
				? `consecutive failures reached the limit (${paused.consecutive_failures})`
				: paused.reason === "turn_budget"
					? `turn budget reached (${paused.turns_used})`
					: "paused";
		return `Objective paused automatically: ${why}. Queued follow-ups stop until you resume it.`;
	}
	return "No payload summary";
}

function eventSignature(event: CoreEvent): string {
	return JSON.stringify(event);
}

function occurrenceSignature(
	event: CoreEvent,
	counts: Map<string, number>,
) {
	const base = eventSignature(event);
	const nextCount = (counts.get(base) ?? 0) + 1;
	counts.set(base, nextCount);
	return `${base}#${nextCount}`;
}

function pushHistoryEvent(
	events: TimelineEvent[],
	seen: Set<string>,
	counts: Map<string, number>,
	record: SessionEventRecord,
) {
	const coreEvent = recordToCoreEvent(record);
	if (!coreEvent) {
		return;
	}

	const signature = occurrenceSignature(coreEvent, counts);
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
	counts: Map<string, number>,
	event: CoreEvent,
) {
	const signature = occurrenceSignature(event, counts);
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
	// A missing session means there is no thread to project. Treating null as
	// "all sessions" leaks messages and active plans while switching workspaces
	// or opening a brand-new workspace.
	if (sessionId === null) {
		return [];
	}

	const merged: TimelineEvent[] = [];
	const seen = new Set<string>();
	const historyCounts = new Map<string, number>();
	const liveCounts = new Map<string, number>();

	for (const record of historyEvents) {
		if (record.sessionId !== sessionId) {
			continue;
		}
		pushHistoryEvent(merged, seen, historyCounts, record);
	}

	for (const event of liveEvents) {
		if (getEventSessionId(event) !== sessionId) {
			continue;
		}
		pushLiveEvent(merged, seen, liveCounts, event);
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

function ensureAssistantItemMessage(
	messages: WorkspaceMessage[],
	assistantItems: Map<string, WorkspaceMessage>,
	assistantBuckets: Map<string, WorkspaceMessage>,
	assistantMessagesByTurn: Map<string, WorkspaceMessage[]>,
	sessionId: string,
	turnId: string,
	messageId: string,
	phase: AssistantMessagePhase,
	makeCurrent: boolean,
	createdAt?: string,
) {
	const itemKey = `${turnId}\u0000${messageId}`;
	let message = assistantItems.get(itemKey);
	if (message) {
		if (phase !== "unknown") {
			message.assistantPhase = phase;
		}
		if (makeCurrent) {
			assistantBuckets.set(turnId, message);
		}
		return message;
	}

	message = {
		id: `assistant-${sessionId}-${turnId}-${messageId}`,
		role: "assistant",
		turnId,
		assistantPhase: phase,
		label: "Assistant",
		content: "",
		streaming: true,
		createdAt,
	};
	assistantItems.set(itemKey, message);
	const turnMessages = assistantMessagesByTurn.get(turnId) ?? [];
	const previousBucket = assistantBuckets.get(turnId);
	if (previousBucket && !turnMessages.includes(previousBucket)) {
		turnMessages.push(previousBucket);
	}
	turnMessages.push(message);
	assistantMessagesByTurn.set(turnId, turnMessages);
	if (makeCurrent) {
		assistantBuckets.set(turnId, message);
	}
	messages.push(message);
	return message;
}

function assistantPhasesAreCompatible(
	activePhase: AssistantMessagePhase | undefined,
	completedPhase: AssistantMessagePhase,
) {
	return (
		!activePhase ||
		activePhase === "unknown" ||
		completedPhase === "unknown" ||
		activePhase === completedPhase
	);
}

function sameNativeSubagent(
	left: Extract<WorkspaceMessageAnnotation, { type: "native-subagent" }>,
	right: Extract<WorkspaceMessageAnnotation, { type: "native-subagent" }>,
) {
	return (
		left.id === right.id ||
		(Boolean(right.agentThreadId) && left.agentThreadId === right.agentThreadId) ||
		(Boolean(right.agentId) && left.agentId === right.agentId)
	);
}

function mergeTurnAnnotation(
	annotations: WorkspaceMessageAnnotation[],
	incoming: WorkspaceMessageAnnotation,
) {
	const existingIndex = annotations.findIndex((existing) => {
		if (existing.type !== incoming.type) return false;
		if (existing.type === "native-subagent" && incoming.type === "native-subagent") {
			return sameNativeSubagent(existing, incoming);
		}
		return existing.id === incoming.id;
	});
	if (existingIndex < 0) {
		annotations.push({ ...incoming });
		return;
	}
	const existing = annotations[existingIndex];
	if (existing?.type === "native-subagent" && incoming.type === "native-subagent") {
		const definedIncoming = Object.fromEntries(
			Object.entries(incoming).filter(([, value]) => value !== undefined),
		) as Partial<typeof incoming>;
		annotations[existingIndex] = {
			...existing,
			...definedIncoming,
			// The first identity is the React key already visible to the user.
			id: existing.id,
		};
		return;
	}
	annotations[existingIndex] = {
		...existing,
		...incoming,
	} as WorkspaceMessageAnnotation;
}

/**
 * Projects every structured provider turn as one stable assistant row.
 * Commentary stays in the activity disclosure while the final answer, when
 * the provider identifies one, streams below it. Providers without native
 * phases keep all live text in activity and reveal the last item on settle.
 */
function foldAssistantTurnMessages(
	messages: WorkspaceMessage[],
	assistantMessagesByTurn: Map<string, WorkspaceMessage[]>,
	settledTurns: ReadonlySet<string>,
	sessionId: string,
) {
	const hidden = new Set<WorkspaceMessage>();
	for (const [turnId, turnMessages] of assistantMessagesByTurn) {
		if (turnMessages.length === 0) {
			continue;
		}
		const settled = settledTurns.has(turnId);
		const finalAnswer = [...turnMessages]
			.reverse()
			.find((message) => message.assistantPhase === "final_answer");
		const terminal = settled
			? finalAnswer ??
				[...turnMessages]
					.reverse()
					.find((message) => message.content.trim().length > 0) ??
				turnMessages.at(-1)
			: finalAnswer ?? turnMessages.at(-1);
		if (!terminal) {
			continue;
		}

		const foldedAnnotations: WorkspaceMessageAnnotation[] = [];
		for (const message of turnMessages) {
			const isVisibleAnswer =
				message === terminal &&
				(settled || message.assistantPhase === "final_answer");
			if (!isVisibleAnswer && message.content.trim().length > 0) {
				mergeTurnAnnotation(foldedAnnotations, {
					type: "commentary",
					id: `commentary-${message.id}`,
					content: message.content,
					streaming:
						!settled && message === terminal
							? true
							: message.streaming,
					createdAt: message.createdAt,
				});
			}
			for (const annotation of message.annotations ?? []) {
				mergeTurnAnnotation(foldedAnnotations, annotation);
			}
			if (message !== terminal) hidden.add(message);
		}
		terminal.id = `assistant-${sessionId}-${turnId}`;
		terminal.turnId = turnId;
		terminal.streaming = !settled;
		terminal.annotations = foldedAnnotations.length > 0 ? foldedAnnotations : undefined;
		if (!settled && terminal.assistantPhase !== "final_answer") {
			terminal.content = "";
		}
	}
	return hidden.size > 0 ? messages.filter((message) => !hidden.has(message)) : messages;
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

function getOrCreateNativeSubagentAnnotation(
	message: WorkspaceMessage,
	annotation: Extract<WorkspaceMessageAnnotation, { type: "native-subagent" }>,
) {
	const annotations = message.annotations ?? (message.annotations = []);
	const existingIndex = annotations.findIndex((item) => {
		if (item.type !== "native-subagent") {
			return false;
		}
		return (
			item.id === annotation.id ||
			(Boolean(annotation.agentThreadId) && item.agentThreadId === annotation.agentThreadId) ||
			(Boolean(annotation.agentId) && item.agentId === annotation.agentId)
		);
	});
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
	const assistantItems = new Map<string, WorkspaceMessage>();
	const assistantMessagesByTurn = new Map<string, WorkspaceMessage[]>();
	const activeAssistantItemsByTurn = new Map<string, Set<WorkspaceMessage>>();
	const completedAssistantItems = new Set<string>();
	const completedReasoningItems = new Set<string>();
	const completedToolCallItems = new Set<string>();
	const delegationBuckets = new Map<string, WorkspaceMessage>();
	const completedTurns = new Set<string>();
	const abortedTurns = new Map<string, string>();
	const turnStartedAtByTurnId = new Map<string, string>();
	const turnModelByTurnId = new Map<string, string | null>();
	const requestedSubagentModels = new Map<string, string>();
	const confirmedSubagentModels = new Map<string, string>();
	const filteredEvents = mergeSessionThreadEvents(historyEvents, liveEvents, sessionId);

	for (const timelineEvent of filteredEvents) {
		const event = timelineEvent.event;
		const occurredAt = timelineEvent.occurredAt;

		if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
			turnModelByTurnId.set(
				event.sessionTurnStarted.turn_id,
				event.sessionTurnStarted.model ?? null,
			);
			turnStartedAtByTurnId.set(
				event.sessionTurnStarted.turn_id,
				occurredAt ?? "",
			);
			messages.push({
				id: `user-${event.sessionTurnStarted.session_id}-${event.sessionTurnStarted.turn_id}`,
				role: "user",
				turnId: event.sessionTurnStarted.turn_id,
				label: "User",
				content: event.sessionTurnStarted.prompt,
				createdAt: occurredAt,
				planMode: event.sessionTurnStarted.plan_mode === true,
				...(event.sessionTurnStarted.evidence
					? { evidence: event.sessionTurnStarted.evidence }
					: {}),
				...(event.sessionTurnStarted.retry_of_turn_id
					? { retryOfTurnId: event.sessionTurnStarted.retry_of_turn_id }
					: {}),
			});
			continue;
		}

		if ("sessionTurnSteered" in event && event.sessionTurnSteered) {
			messages.push({
				id: `user-steer-${event.sessionTurnSteered.session_id}-${event.sessionTurnSteered.turn_id}-${timelineEvent.signature}`,
				role: "user",
				label: "User",
				content: event.sessionTurnSteered.prompt,
				createdAt: occurredAt,
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

		if (
			"sessionTurnAssistantMessageStarted" in event &&
			event.sessionTurnAssistantMessageStarted
		) {
			const item = event.sessionTurnAssistantMessageStarted;
			const itemKey = `${item.turn_id}\u0000${item.message_id}`;
			if (completedAssistantItems.has(itemKey)) continue;
			const message = ensureAssistantItemMessage(
				messages,
				assistantItems,
				assistantBuckets,
				assistantMessagesByTurn,
				item.session_id,
				item.turn_id,
				item.message_id,
				item.phase,
				true,
				turnStartedAtByTurnId.get(item.turn_id) ?? occurredAt,
			);
			const activeItems = activeAssistantItemsByTurn.get(item.turn_id) ?? new Set();
			activeItems.add(message);
			activeAssistantItemsByTurn.set(item.turn_id, activeItems);
			continue;
		}

		if (
			"sessionTurnAssistantMessageDelta" in event &&
			event.sessionTurnAssistantMessageDelta
		) {
			const item = event.sessionTurnAssistantMessageDelta;
			const itemKey = `${item.turn_id}\u0000${item.message_id}`;
			if (completedAssistantItems.has(itemKey)) continue;
			const message = ensureAssistantItemMessage(
				messages,
				assistantItems,
				assistantBuckets,
				assistantMessagesByTurn,
				item.session_id,
				item.turn_id,
				item.message_id,
				"unknown",
				true,
				turnStartedAtByTurnId.get(item.turn_id) ?? occurredAt,
			);
			message.content = `${message.content}${item.content}`;
			message.streaming = !completedTurns.has(item.turn_id) && !abortedTurns.has(item.turn_id);
			const activeItems = activeAssistantItemsByTurn.get(item.turn_id) ?? new Set();
			activeItems.add(message);
			activeAssistantItemsByTurn.set(item.turn_id, activeItems);
			continue;
		}

		if (
			"sessionTurnAssistantMessageCompleted" in event &&
			event.sessionTurnAssistantMessageCompleted
		) {
			const item = event.sessionTurnAssistantMessageCompleted;
			const itemKey = `${item.turn_id}\u0000${item.message_id}`;
			const exactMessage = assistantItems.get(itemKey);
			const activeItems = activeAssistantItemsByTurn.get(item.turn_id);
			const soleActiveMessage =
				!exactMessage && activeItems?.size === 1 ? [...activeItems][0] : undefined;
			const lifecycleMessage =
				soleActiveMessage &&
				assistantPhasesAreCompatible(soleActiveMessage.assistantPhase, item.phase)
					? soleActiveMessage
					: undefined;
			const message =
				exactMessage ??
				lifecycleMessage ??
				ensureAssistantItemMessage(
					messages,
					assistantItems,
					assistantBuckets,
					assistantMessagesByTurn,
					item.session_id,
					item.turn_id,
					item.message_id,
					item.phase,
					false,
					turnStartedAtByTurnId.get(item.turn_id) ?? occurredAt,
				);
			if (lifecycleMessage) {
				// Persisted histories from older provider adapters may finish a
				// streamed root message under the authoritative snapshot ID. Alias
				// that terminal ID only when one compatible item is active, so real
				// native items and concurrent lifecycles remain distinct.
				assistantItems.set(itemKey, lifecycleMessage);
			}
			message.assistantPhase = item.phase;
			if (item.content !== null) {
				message.content = item.content;
			}
			message.streaming = false;
			// Completion can alias an authoritative provider ID to the original
			// streaming envelope. Mark every key for that lifecycle as terminal so
			// delayed deltas under either ID cannot reopen or duplicate the message.
			for (const [candidateKey, candidate] of assistantItems) {
				if (candidate === message) completedAssistantItems.add(candidateKey);
			}
			activeItems?.delete(message);
			continue;
		}

		if ("sessionTurnReasoningStarted" in event && event.sessionTurnReasoningStarted) {
			const key = event.sessionTurnReasoningStarted.turn_id;
			const itemKey = `${key}\u0000${event.sessionTurnReasoningStarted.reasoning_id}`;
			if (completedReasoningItems.has(itemKey)) continue;
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
			const itemKey = `${key}\u0000${event.sessionTurnReasoningDelta.reasoning_id}`;
			if (completedReasoningItems.has(itemKey)) continue;
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
			completedReasoningItems.add(
				`${key}\u0000${event.sessionTurnReasoningCompleted.reasoning_id}`,
			);
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
			const itemKey = `${key}\u0000${event.sessionTurnToolCallStarted.tool_call_id}`;
			if (completedToolCallItems.has(itemKey)) continue;
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
			const itemKey = `${key}\u0000${event.sessionTurnToolCallDelta.tool_call_id}`;
			if (completedToolCallItems.has(itemKey)) continue;
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
			completedToolCallItems.add(
				`${key}\u0000${event.sessionTurnToolCallCompleted.tool_call_id}`,
			);
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
			completedToolCallItems.add(
				`${key}\u0000${event.sessionTurnToolCallFailed.tool_call_id}`,
			);
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

		if (
			"sessionTurnNativeSubagentActivity" in event &&
			event.sessionTurnNativeSubagentActivity
		) {
			const item = event.sessionTurnNativeSubagentActivity;
			const key = item.turn_id;
			const message = ensureAssistantMessage(
				messages,
				assistantBuckets,
				item.session_id,
				key,
				turnStartedAtByTurnId.get(key) ?? occurredAt,
			);
			const annotation = getOrCreateNativeSubagentAnnotation(message, {
				type: "native-subagent",
				id: item.id,
				agentId: item.agent_id ?? undefined,
				agentThreadId: item.agent_thread_id ?? undefined,
				path: item.path ?? undefined,
				name: item.name ?? undefined,
				role: item.role ?? undefined,
				requestedModel: requestedSubagentModels.get(item.agent_thread_id ?? item.id),
				model: confirmedSubagentModels.get(item.agent_thread_id ?? item.id) ?? item.model ?? undefined,
				status: item.status,
				createdAt: occurredAt,
			});
			if (annotation.type === "native-subagent") {
				annotation.agentId = item.agent_id ?? annotation.agentId;
				annotation.agentThreadId = item.agent_thread_id ?? annotation.agentThreadId;
				annotation.path = item.path ?? annotation.path;
				annotation.name = item.name ?? annotation.name;
				annotation.role = item.role ?? annotation.role;
				annotation.model = item.model ?? annotation.model;
				annotation.model = confirmedSubagentModels.get(item.agent_thread_id ?? item.id) ?? annotation.model;
				annotation.requestedModel = requestedSubagentModels.get(item.agent_thread_id ?? item.id) ?? annotation.requestedModel;
				annotation.status = item.status;
				annotation.createdAt ??= occurredAt;
			}
			continue;
		}

		if ("sessionTurnNativeSubagentModelRequested" in event && event.sessionTurnNativeSubagentModelRequested) {
			const item = event.sessionTurnNativeSubagentModelRequested;
			requestedSubagentModels.set(item.correlation_id, item.model);
			const annotation = assistantBuckets.get(item.turn_id)?.annotations?.find(
				(candidate) => candidate.type === "native-subagent" && (candidate.agentThreadId === item.correlation_id || candidate.id === item.correlation_id),
			);
			if (annotation?.type === "native-subagent") annotation.requestedModel = item.model;
			continue;
		}

		if ("sessionTurnNativeSubagentModelConfirmed" in event && event.sessionTurnNativeSubagentModelConfirmed) {
			const item = event.sessionTurnNativeSubagentModelConfirmed;
			confirmedSubagentModels.set(item.correlation_id, item.model);
			const annotation = assistantBuckets.get(item.turn_id)?.annotations?.find(
				(candidate) => candidate.type === "native-subagent" && (candidate.agentThreadId === item.correlation_id || candidate.id === item.correlation_id),
			);
			if (annotation?.type === "native-subagent") annotation.model = item.model;
			continue;
		}

		if ("sessionTurnModelEffective" in event && event.sessionTurnModelEffective) {
			turnModelByTurnId.set(event.sessionTurnModelEffective.turn_id, event.sessionTurnModelEffective.model);
			continue;
		}

		if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
			const key = event.sessionTurnCompleted.turn_id;
			completedTurns.add(key);
			const turnMessages = assistantMessagesByTurn.get(key) ?? [assistantBuckets.get(key)].filter(
				(message): message is WorkspaceMessage => Boolean(message),
			);
			for (const existing of turnMessages) {
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
			let turnMessages = assistantMessagesByTurn.get(key) ?? [assistantBuckets.get(key)].filter(
				(message): message is WorkspaceMessage => Boolean(message),
			);
			if (turnMessages.length === 0 && turnStartedAtByTurnId.has(key)) {
				// A turn that died before any output (provider crash, DCC restart)
				// still needs a visible place for its reason and retry/continue.
				turnMessages = [
					ensureAssistantMessage(
						messages,
						assistantBuckets,
						event.sessionTurnAborted.session_id,
						key,
						turnStartedAtByTurnId.get(key) ?? occurredAt,
					),
				];
			}
			for (const existing of turnMessages) {
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

		if ("sessionDelegationRequested" in event || "sessionDelegationStarted" in event || "sessionDelegationDelta" in event || "sessionDelegationCompleted" in event || "sessionDelegationFailed" in event || "sessionDelegationCancelled" in event) {
			const delegationId =
				("sessionDelegationRequested" in event &&
					event.sessionDelegationRequested?.delegation_id) ||
				("sessionDelegationStarted" in event &&
					event.sessionDelegationStarted?.delegation_id) ||
				("sessionDelegationDelta" in event &&
					event.sessionDelegationDelta?.delegation_id) ||
				("sessionDelegationCompleted" in event &&
					event.sessionDelegationCompleted?.delegation_id) ||
				("sessionDelegationFailed" in event &&
					event.sessionDelegationFailed?.delegation_id) ||
				("sessionDelegationCancelled" in event &&
					event.sessionDelegationCancelled?.delegation_id) ||
				null;
			if (!delegationId) {
				continue;
			}

			let message = delegationBuckets.get(delegationId);
			if (!message) {
				message = {
					id: `delegation-${delegationId}`,
					role: "system",
					label: "delegation",
					content: eventSummary(event),
					createdAt: occurredAt,
					delegation: {
						id: delegationId,
						phase: "requested",
					},
				};
				delegationBuckets.set(delegationId, message);
				messages.push(message);
			}
			const delegation = message.delegation;
			if (!delegation) {
				continue;
			}

			if ("sessionDelegationStarted" in event && event.sessionDelegationStarted) {
				delegation.phase = "running";
				delegation.childSessionId =
					event.sessionDelegationStarted.child_session_id ??
					delegation.childSessionId;
				if (delegation.childSessionId) {
					message.action = {
						type: "open-session",
						sessionId: delegation.childSessionId,
						label: "Open child session",
					};
				}
				message.content = eventSummary(event);
			} else if ("sessionDelegationDelta" in event && event.sessionDelegationDelta) {
				message.content = event.sessionDelegationDelta.content;
			} else if (
				"sessionDelegationCompleted" in event &&
				event.sessionDelegationCompleted
			) {
				delegation.phase = "completed";
				delegation.summary = event.sessionDelegationCompleted.summary ?? null;
				message.content = eventSummary(event);
			} else if ("sessionDelegationFailed" in event && event.sessionDelegationFailed) {
				delegation.phase = "failed";
				delegation.reason = event.sessionDelegationFailed.reason ?? null;
				message.content = eventSummary(event);
			} else if (
				"sessionDelegationCancelled" in event &&
				event.sessionDelegationCancelled
			) {
				delegation.phase = "cancelled";
				delegation.reason = event.sessionDelegationCancelled.reason ?? null;
				message.content = eventSummary(event);
			}
			continue;
		}

		if ("sessionCompleted" in event || "sessionAborted" in event || "sessionResumed" in event || "sessionObjectivePaused" in event || "sessionCheckpointCreated" in event || "workspacePrepared" in event || "workspaceReady" in event) {
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
	for (const [turnId, model] of turnModelByTurnId) {
		if (!model) continue;
		for (const message of assistantMessagesByTurn.get(turnId) ?? []) {
			message.model = model;
		}
		const message = assistantBuckets.get(turnId);
		if (message) message.model = model;
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

	return foldAssistantTurnMessages(
		messages,
		assistantMessagesByTurn,
		new Set([...completedTurns, ...abortedTurns.keys()]),
		sessionId ?? "unknown-session",
	);
}
