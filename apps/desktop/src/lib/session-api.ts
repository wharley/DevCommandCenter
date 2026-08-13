import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SESSION_METHODS } from "@dcc/contracts";
import type {
	CoreEvent,
	ListMcpRuntimeStatusesOutput,
	McpRuntimeStatus,
	PrepareTurnOutput,
	SessionEventRecord,
	SessionSearchResult,
	WorkspaceSessionSummary,
} from "@dcc/contracts";
import type {
	AbortRunInput,
	AbortRunOutput,
	ApplyTaskTitleInput,
	ApplyTaskTitleOutput,
	ApprovePlanInput,
	ApprovePlanOutput,
	CloseSessionInput,
	CloseSessionOutput,
	RecordPlanHandoffInput,
	RecordPlanHandoffOutput,
	RunPullRequestReviewAgentInput,
	RunPullRequestReviewAgentOutput,
	RespondToUserInputInput,
	RespondToUserInputOutput,
	RespondToPermissionRequestInput,
	RespondToPermissionRequestOutput,
	RestoreSessionInput,
	RestoreSessionOutput,
	ResumeSessionInput,
	ResumeSessionOutput,
	SendTurnInput,
	SendTurnOutput,
	SteerTurnInput,
	SteerTurnOutput,
	QueueTurnInput,
	QueuedTurn,
	RemoveQueuedTurnInput,
	ReorderTurnQueueInput,
	StartMcpOauthInput,
	StartMcpOauthOutput,
	StartThreadInput,
	StartThreadOutput,
	WaitMcpOauthOutput,
} from "@dcc/contracts";
import { resolveMcpTurnPreflight } from "./mcp-turn-preflight";
import { openExternal } from "./shell-api";

// Re-exported only because a few consumers still listen for it. Safe to drop
// once the remote-core-event listeners are removed.
export const REMOTE_CORE_EVENT_NAME = "dcc:remote-core-event";

export function startThread(input: StartThreadInput) {
	return invoke<StartThreadOutput>(SESSION_METHODS.startThread, { input });
}

export function runPullRequestReviewAgent(input: RunPullRequestReviewAgentInput) {
	return invoke<RunPullRequestReviewAgentOutput>(SESSION_METHODS.runPullRequestReviewAgent, { input });
}

export function applyTaskTitle(input: ApplyTaskTitleInput) {
	return invoke<ApplyTaskTitleOutput>(SESSION_METHODS.applyTaskTitle, { input });
}

function prepareTurn(input: SendTurnInput) {
	return invoke<PrepareTurnOutput>(SESSION_METHODS.prepareTurn, { input });
}

function waitMcpOauth(sessionId: string, definitionId: string) {
	return invoke<WaitMcpOauthOutput>(SESSION_METHODS.waitMcpOauth, {
		input: { sessionId, definitionId },
	});
}

export async function sendTurn(input: SendTurnInput) {
	await resolveMcpTurnPreflight(input, {
		prepareTurn,
		openAuthorizationUrl: openExternal,
		waitForOauth: ({ sessionId, definitionId }) =>
			waitMcpOauth(sessionId, definitionId),
	});
	return invoke<SendTurnOutput>(SESSION_METHODS.sendTurn, { input });
}

export function steerTurn(input: SteerTurnInput) {
	return invoke<SteerTurnOutput>(SESSION_METHODS.steerTurn, { input });
}

export function queueTurn(input: QueueTurnInput) {
	return invoke<QueuedTurn>(SESSION_METHODS.queueTurn, { input });
}

export function loadTurnQueue(sessionId: string) {
	return invoke<QueuedTurn[]>(SESSION_METHODS.listTurnQueue, { sessionId });
}

export function removeQueuedTurn(input: RemoveQueuedTurnInput) {
	return invoke<QueuedTurn[]>(SESSION_METHODS.removeQueuedTurn, { input });
}

export function reorderTurnQueue(input: ReorderTurnQueueInput) {
	return invoke<QueuedTurn[]>(SESSION_METHODS.reorderTurnQueue, { input });
}

export function dispatchNextQueuedTurn(sessionId: string) {
	return invoke<boolean>(SESSION_METHODS.dispatchNextQueuedTurn, { sessionId });
}

export function abortRun(input: AbortRunInput) {
	return invoke<AbortRunOutput>(SESSION_METHODS.abortRun, { input });
}

export function approvePlan(input: ApprovePlanInput) {
	return invoke<ApprovePlanOutput>(SESSION_METHODS.approvePlan, { input });
}

export function recordPlanHandoff(input: RecordPlanHandoffInput) {
	return invoke<RecordPlanHandoffOutput>(SESSION_METHODS.recordPlanHandoff, {
		input,
	});
}

export function resumeSession(input: ResumeSessionInput) {
	return invoke<ResumeSessionOutput>(SESSION_METHODS.resumeSession, { input });
}

export function closeSession(input: CloseSessionInput) {
	return invoke<CloseSessionOutput>(SESSION_METHODS.closeSession, { input });
}

export function restoreSession(input: RestoreSessionInput) {
	return invoke<RestoreSessionOutput>(SESSION_METHODS.restoreSession, { input });
}

export function respondToUserInput(input: RespondToUserInputInput) {
	return invoke<RespondToUserInputOutput>(SESSION_METHODS.respondToUserInput, {
		input,
	});
}

export function respondToPermissionRequest(input: RespondToPermissionRequestInput) {
	return invoke<RespondToPermissionRequestOutput>(
		SESSION_METHODS.respondToPermissionRequest,
		{ input },
	);
}

export function loadSessionThreadEvents(sessionId: string) {
	return invoke<SessionEventRecord[]>(SESSION_METHODS.listThreadEvents, {
		sessionId,
	});
}

export function loadMcpRuntimeStatuses(sessionId: string) {
	return invoke<ListMcpRuntimeStatusesOutput>(
		SESSION_METHODS.listMcpRuntimeStatuses,
		{ input: { sessionId } },
	);
}

export function startMcpOauth(input: StartMcpOauthInput) {
	return invoke<StartMcpOauthOutput>(SESSION_METHODS.startMcpOauth, { input });
}

export async function listenMcpRuntimeStatusEvents(
	handler: (event: {
		sessionId: string;
		statuses: McpRuntimeStatus[];
	}) => void,
) {
	return listen<CoreEvent>(
		"dcc/session/mcp/runtime-status",
		({ payload }) => {
			if (
				"sessionMcpRuntimeStatusChanged" in payload &&
				payload.sessionMcpRuntimeStatusChanged
			) {
				handler({
					sessionId:
						payload.sessionMcpRuntimeStatusChanged.session_id,
					statuses: payload.sessionMcpRuntimeStatusChanged.statuses,
				});
			}
		},
	);
}

export function loadWorkspaceSessions(workspaceId: string) {
	return invoke<WorkspaceSessionSummary[]>(
		SESSION_METHODS.listWorkspaceSessions,
		{ workspaceId },
	);
}

export function searchSessionHistory(query: string, limit = 40) {
	return invoke<SessionSearchResult[]>(SESSION_METHODS.searchSessions, {
		input: { query, limit },
	});
}

export const SESSION_EVENT_NAMES = [
	"dcc/session/started",
	"dcc/session/completed",
	"dcc/session/aborted",
	"dcc/session/resumed",
	"dcc/session/mcp/runtime-status",
	"dcc/session/turn/started",
	"dcc/session/turn/steered",
	"dcc/session/turn/queued",
	"dcc/session/turn/queue/removed",
	"dcc/session/turn/queue/reordered",
	"dcc/session/turn/queue/dispatched",
	"dcc/session/turn/delta",
	"dcc/session/turn/assistant-message/started",
	"dcc/session/turn/assistant-message/delta",
	"dcc/session/turn/assistant-message/completed",
	"dcc/session/turn/reasoning/started",
	"dcc/session/turn/reasoning/delta",
	"dcc/session/turn/reasoning/completed",
	"dcc/session/turn/tool-call/started",
	"dcc/session/turn/tool-call/delta",
	"dcc/session/turn/tool-call/completed",
	"dcc/session/turn/tool-call/failed",
	"dcc/session/turn/user-input/requested",
	"dcc/session/turn/user-input/resolved",
	"dcc/session/turn/permission/requested",
	"dcc/session/turn/permission/resolved",
	"dcc/session/turn/native-subagent/activity",
	"dcc/session/turn/native-subagent/model-requested",
	"dcc/session/turn/native-subagent/model-confirmed",
	"dcc/session/turn/model-effective",
	"dcc/session/turn/completed",
	"dcc/session/turn/aborted",
	"dcc/session/checkpoint/created",
	"dcc/session/plan/approved",
	"dcc/session/plan/handed-off",
	"dcc/session/delegation/requested",
	"dcc/session/delegation/started",
	"dcc/session/delegation/delta",
	"dcc/session/delegation/completed",
	"dcc/session/delegation/failed",
	"dcc/session/delegation/cancelled",
] as const;

export async function listenSessionEvents(
	handler: (event: CoreEvent) => void,
) {
	const unlistenFns = await Promise.all(
		SESSION_EVENT_NAMES.map((eventName) =>
			listen<CoreEvent>(eventName, (event) => {
				handler(event.payload);
			}),
		),
	);

	return () => {
		for (const unlisten of unlistenFns) {
			void unlisten();
		}
	};
}
