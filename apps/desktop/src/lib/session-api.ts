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
import {
	getActiveRemoteEnvironment,
	type SavedRemoteEnvironment,
} from "@/features/settings/remote-environments-store";

type RemoteSessionBackend =
	| { kind: "local" }
	| { kind: "remote"; environment: SavedRemoteEnvironment };

function getSessionBackendTarget(): RemoteSessionBackend {
	const environment = getActiveRemoteEnvironment();
	if (environment?.endpoint && environment.bearerToken) {
		return { kind: "remote", environment };
	}
	return { kind: "local" };
}

async function remoteSessionRequest<T>(
	environment: SavedRemoteEnvironment,
	path: string,
	init?: RequestInit,
): Promise<T> {
	const endpoint = environment.endpoint?.trim();
	const bearerToken = environment.bearerToken?.trim();
	if (!endpoint || !bearerToken) {
		throw new Error("Remote environment is missing endpoint or bearer token.");
	}

	const url = new URL(path, endpoint.endsWith("/") ? endpoint : `${endpoint}/`);
	const response = await fetch(url, {
		...init,
		headers: {
			Authorization: `Bearer ${bearerToken}`,
			"Content-Type": "application/json",
			...(init?.headers ?? {}),
		},
	});
	if (!response.ok) {
		let message = `Remote session API returned HTTP ${response.status}`;
		try {
			const payload = (await response.json()) as {
				error?: { message?: string } | string;
			};
			if (typeof payload.error === "string") {
				message = payload.error;
			} else if (payload.error?.message) {
				message = payload.error.message;
			}
		} catch {
			/* keep default message */
		}
		throw new Error(message);
	}
	return (await response.json()) as T;
}

export function startThread(input: StartThreadInput) {
	return invoke<StartThreadOutput>(SESSION_METHODS.startThread, { input });
}

export function sendTurn(input: SendTurnInput) {
	return invoke<SendTurnOutput>(SESSION_METHODS.sendTurn, { input });
}

export function abortRun(input: AbortRunInput) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<AbortRunOutput>(
			target.environment,
			`/api/v1/sessions/${encodeURIComponent(input.sessionId)}/abort`,
			{ method: "POST" },
		);
	}
	return invoke<AbortRunOutput>(SESSION_METHODS.abortRun, { input });
}

export function resumeSession(input: ResumeSessionInput) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<ResumeSessionOutput>(
			target.environment,
			`/api/v1/sessions/${encodeURIComponent(input.sessionId)}/resume`,
			{ method: "POST" },
		);
	}
	return invoke<ResumeSessionOutput>(SESSION_METHODS.resumeSession, { input });
}

export function closeSession(input: CloseSessionInput) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<CloseSessionOutput>(
			target.environment,
			`/api/v1/sessions/${encodeURIComponent(input.sessionId)}/close`,
			{
				method: "POST",
				body: JSON.stringify({ deleteHistory: input.deleteHistory }),
			},
		);
	}
	return invoke<CloseSessionOutput>(SESSION_METHODS.closeSession, { input });
}

export function restoreSession(input: RestoreSessionInput) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<RestoreSessionOutput>(
			target.environment,
			`/api/v1/sessions/${encodeURIComponent(input.sessionId)}/restore`,
			{ method: "POST" },
		);
	}
	return invoke<RestoreSessionOutput>(SESSION_METHODS.restoreSession, { input });
}

export function respondToUserInput(input: RespondToUserInputInput) {
	return invoke<RespondToUserInputOutput>(SESSION_METHODS.respondToUserInput, { input });
}

export function respondToPermissionRequest(input: RespondToPermissionRequestInput) {
	return invoke<RespondToPermissionRequestOutput>(
		SESSION_METHODS.respondToPermissionRequest,
		{ input },
	);
}

export function loadSessionThreadEvents(sessionId: string) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<SessionEventRecord[]>(
			target.environment,
			`/api/v1/sessions/${encodeURIComponent(sessionId)}/events`,
		);
	}
	return invoke<SessionEventRecord[]>(SESSION_METHODS.listThreadEvents, {
		sessionId,
	});
}

export function loadWorkspaceSessions(workspaceId: string) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<WorkspaceSessionSummary[]>(
			target.environment,
			`/api/v1/sessions?workspaceId=${encodeURIComponent(workspaceId)}`,
		);
	}
	return invoke<WorkspaceSessionSummary[]>(
		SESSION_METHODS.listWorkspaceSessions,
		{ workspaceId },
	);
}

export function searchSessionHistory(query: string, limit = 40) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<SessionSearchResult[]>(
			target.environment,
			`/api/v1/sessions/search?query=${encodeURIComponent(query)}&limit=${encodeURIComponent(String(limit))}`,
		);
	}
	return invoke<SessionSearchResult[]>(SESSION_METHODS.searchSessions, {
		input: {
			query,
			limit,
		},
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
] as const;

export async function listenSessionEvents(
	handler: (event: CoreEvent) => void,
) {
	const unlistenFns = await Promise.all(
		SESSION_EVENT_NAMES.map((eventName) => listen<CoreEvent>(eventName, (event) => {
			handler(event.payload);
		})),
	);

	return () => {
		for (const unlisten of unlistenFns) {
			void unlisten();
		}
	};
}
