import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SESSION_METHODS } from "@dcc/contracts";
import type {
	CoreEvent,
	SessionEventRecord,
	SessionSearchResult,
	WorkspaceSessionSummary,
} from "@dcc/contracts";
import type {
	AbortRunInput,
	AbortRunOutput,
	CloseSessionInput,
	CloseSessionOutput,
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
	StartThreadInput,
	StartThreadOutput,
} from "@dcc/contracts";

// Re-exported only because a few consumers still listen for it. Safe to drop
// once the remote-core-event listeners are removed.
export const REMOTE_CORE_EVENT_NAME = "dcc:remote-core-event";

export function startThread(input: StartThreadInput) {
	return invoke<StartThreadOutput>(SESSION_METHODS.startThread, { input });
}

export function sendTurn(input: SendTurnInput) {
	return invoke<SendTurnOutput>(SESSION_METHODS.sendTurn, { input });
}

export function abortRun(input: AbortRunInput) {
	return invoke<AbortRunOutput>(SESSION_METHODS.abortRun, { input });
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

const SESSION_EVENT_NAMES = [
	"dcc/session/started",
	"dcc/session/completed",
	"dcc/session/aborted",
	"dcc/session/resumed",
	"dcc/session/turn/started",
	"dcc/session/turn/delta",
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
	"dcc/session/turn/completed",
	"dcc/session/turn/aborted",
	"dcc/session/checkpoint/created",
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
