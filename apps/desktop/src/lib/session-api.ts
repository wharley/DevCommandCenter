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

export const REMOTE_CORE_EVENT_NAME = "dcc:remote-core-event";
const REMOTE_STREAM_RETRY_MS = 1_500;

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

async function listenRemoteSessionEvents(
	environment: SavedRemoteEnvironment,
	handler: (event: CoreEvent) => void,
) {
	const endpoint = environment.endpoint?.trim();
	const bearerToken = environment.bearerToken?.trim();
	if (!endpoint || !bearerToken) {
		throw new Error("Remote environment is missing endpoint or bearer token.");
	}

	const url = new URL("/api/v1/events/stream", endpoint.endsWith("/") ? endpoint : `${endpoint}/`);
	const controller = new AbortController();
	let closed = false;

	const processEventBlock = (block: string) => {
		const dataLines = block
			.split(/\r?\n/u)
			.filter((line) => line.startsWith("data:"))
			.map((line) => line.slice(5).trimStart());
		if (dataLines.length === 0) {
			return;
		}
		try {
			const event = JSON.parse(dataLines.join("\n")) as CoreEvent;
			handler(event);
			if (typeof window !== "undefined") {
				window.dispatchEvent(new CustomEvent(REMOTE_CORE_EVENT_NAME, { detail: event }));
			}
		} catch {
			/* ignore malformed event */
		}
	};

	void (async () => {
		while (!closed) {
			try {
				const response = await fetch(url, {
					method: "GET",
					headers: {
						Authorization: `Bearer ${bearerToken}`,
						Accept: "text/event-stream",
					},
					signal: controller.signal,
				});

				if (!response.ok || !response.body) {
					throw new Error(`Remote event stream returned HTTP ${response.status}`);
				}

				const decoder = new TextDecoder();
				const reader = response.body.getReader();
				let buffer = "";

				try {
					while (!closed) {
						const { value, done } = await reader.read();
						if (done) {
							break;
						}
						buffer += decoder.decode(value, { stream: true });
						let boundary = buffer.search(/\r?\n\r?\n/u);
						while (boundary >= 0) {
							const separatorMatch = buffer
								.slice(boundary)
								.match(/^\r?\n\r?\n/u);
							const separatorLength = separatorMatch?.[0]?.length ?? 2;
							const block = buffer.slice(0, boundary).trim();
							buffer = buffer.slice(boundary + separatorLength);
							if (block.length > 0) {
								processEventBlock(block);
							}
							boundary = buffer.search(/\r?\n\r?\n/u);
						}
					}
				} finally {
					void reader.cancel().catch(() => {});
				}
			} catch (error) {
				if (!controller.signal.aborted) {
					console.error("[dcc] remote event stream failed:", error);
				}
			}

			if (!closed && !controller.signal.aborted) {
				await new Promise((resolve) => {
					window.setTimeout(resolve, REMOTE_STREAM_RETRY_MS);
				});
			}
		}
	})();

	return () => {
		closed = true;
		controller.abort();
	};
}

export function startThread(input: StartThreadInput) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<StartThreadOutput>(
			target.environment,
			"/api/v1/sessions/start",
			{
				method: "POST",
				body: JSON.stringify(input),
			},
		);
	}
	return invoke<StartThreadOutput>(SESSION_METHODS.startThread, { input });
}

export function sendTurn(input: SendTurnInput) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<SendTurnOutput>(
			target.environment,
			`/api/v1/sessions/${encodeURIComponent(input.sessionId)}/turns`,
			{
				method: "POST",
				body: JSON.stringify(input),
			},
		);
	}
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
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<RespondToUserInputOutput>(
			target.environment,
			`/api/v1/sessions/${encodeURIComponent(input.sessionId)}/respond-user-input`,
			{
				method: "POST",
				body: JSON.stringify(input),
			},
		);
	}
	return invoke<RespondToUserInputOutput>(SESSION_METHODS.respondToUserInput, { input });
}

export function respondToPermissionRequest(input: RespondToPermissionRequestInput) {
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return remoteSessionRequest<RespondToPermissionRequestOutput>(
			target.environment,
			`/api/v1/sessions/${encodeURIComponent(input.sessionId)}/respond-permission`,
			{
				method: "POST",
				body: JSON.stringify(input),
			},
		);
	}
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
	const target = getSessionBackendTarget();
	if (target.kind === "remote") {
		return listenRemoteSessionEvents(target.environment, handler);
	}
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
