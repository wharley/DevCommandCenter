import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
	getActiveRemoteEnvironment,
	type SavedRemoteEnvironment,
} from "@/features/settings/remote-environments-store";

export type TerminalSpawnOptions = {
	cwd: string;
	command?: string;
	args?: string[];
	cols?: number;
	rows?: number;
	paneId?: string | null;
	ptyOwnerKey?: string | null;
	restart?: boolean;
};

export type TerminalSpawnResult = {
	ptyId: string;
};

export type TerminalOutputEvent = {
	ptyId: string;
	data: string;
	stream: string;
};

export type TerminalExitEvent = {
	ptyId: string;
	code: number | null;
};

export type ShellDefaultResult = {
	shell: string;
};

type TerminalBackendTarget =
	| { kind: "local" }
	| { kind: "remote"; environment: SavedRemoteEnvironment };

type RemoteTerminalStreamEvent =
	| { kind: "output"; payload: TerminalOutputEvent }
	| { kind: "exit"; payload: TerminalExitEvent };

const TERMINAL_OUTPUT_EVENT = "terminal-output";
const TERMINAL_EXIT_EVENT = "terminal-exit";
const REMOTE_STREAM_RETRY_MS = 1_500;

const remoteOutputListeners = new Set<(event: TerminalOutputEvent) => void>();
const remoteExitListeners = new Set<(event: TerminalExitEvent) => void>();
let remoteBridgeScope: string | null = null;
let remoteBridgePromise: Promise<void> | null = null;
let remoteBridgeCleanup: (() => void) | null = null;

function getTerminalBackendTarget(): TerminalBackendTarget {
	const environment = getActiveRemoteEnvironment();
	if (environment?.endpoint && environment.bearerToken) {
		return { kind: "remote", environment };
	}
	return { kind: "local" };
}

export function getTerminalBackendScope() {
	const target = getTerminalBackendTarget();
	if (target.kind === "remote") {
		return `remote:${target.environment.id}:${target.environment.endpoint}:${target.environment.bearerToken}`;
	}
	return "local";
}

async function remoteTerminalRequest<T>(
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
		let message = `Remote terminal API returned HTTP ${response.status}`;
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

function delay(ms: number) {
	return new Promise((resolve) => {
		window.setTimeout(resolve, ms);
	});
}

async function openRemoteTerminalStream(
	environment: SavedRemoteEnvironment,
	onEvent: (event: RemoteTerminalStreamEvent) => void,
) {
	const endpoint = environment.endpoint?.trim();
	const bearerToken = environment.bearerToken?.trim();
	if (!endpoint || !bearerToken) {
		throw new Error("Remote environment is missing endpoint or bearer token.");
	}

	const url = new URL(
		"/api/v1/terminals/events/stream",
		endpoint.endsWith("/") ? endpoint : `${endpoint}/`,
	);
	const controller = new AbortController();
	let closed = false;

	const processEventBlock = (block: string) => {
		const lines = block.split(/\r?\n/u);
		const eventName =
			lines.find((line) => line.startsWith("event:"))?.slice(6).trim() ?? "message";
		const dataLines = lines
			.filter((line) => line.startsWith("data:"))
			.map((line) => line.slice(5).trimStart());
		if (dataLines.length === 0) {
			return;
		}

		try {
			if (eventName === TERMINAL_OUTPUT_EVENT) {
				onEvent({
					kind: "output",
					payload: JSON.parse(dataLines.join("\n")) as TerminalOutputEvent,
				});
				return;
			}

			if (eventName === TERMINAL_EXIT_EVENT) {
				onEvent({
					kind: "exit",
					payload: JSON.parse(dataLines.join("\n")) as TerminalExitEvent,
				});
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
					throw new Error(`Remote terminal stream returned HTTP ${response.status}`);
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
							const separatorMatch = buffer.slice(boundary).match(/^\r?\n\r?\n/u);
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
					console.error("[dcc] remote terminal stream failed:", error);
				}
			}

			if (!closed && !controller.signal.aborted) {
				await delay(REMOTE_STREAM_RETRY_MS);
			}
		}
	})();

	return () => {
		closed = true;
		controller.abort();
	};
}

async function ensureRemoteTerminalBridge(environment: SavedRemoteEnvironment) {
	const scope = `remote:${environment.id}:${environment.endpoint}:${environment.bearerToken}`;
	if (remoteBridgeScope === scope && remoteBridgePromise) {
		return remoteBridgePromise;
	}

	remoteBridgeCleanup?.();
	remoteBridgeCleanup = null;
	remoteBridgeScope = scope;
	remoteBridgePromise = (async () => {
		remoteBridgeCleanup = await openRemoteTerminalStream(environment, (event) => {
			if (event.kind === "output") {
				for (const listener of remoteOutputListeners) {
					listener(event.payload);
				}
				return;
			}

			for (const listener of remoteExitListeners) {
				listener(event.payload);
			}
		});
	})();
	return remoteBridgePromise;
}

export function getDefaultShell() {
	const target = getTerminalBackendTarget();
	if (target.kind === "remote") {
		return remoteTerminalRequest<ShellDefaultResult>(
			target.environment,
			"/api/v1/shell/default",
			{
				method: "GET",
				headers: {
					Accept: "application/json",
				},
			},
		);
	}
	return invoke<ShellDefaultResult>("shell_get_default");
}

export function spawnTerminal(options: TerminalSpawnOptions) {
	const target = getTerminalBackendTarget();
	if (target.kind === "remote") {
		return remoteTerminalRequest<TerminalSpawnResult>(
			target.environment,
			"/api/v1/terminals/spawn",
			{
				method: "POST",
				body: JSON.stringify(options),
			},
		);
	}
	return invoke<TerminalSpawnResult>("terminal_spawn", { options });
}

export function writeTerminalStdin(ptyId: string, data: string) {
	const target = getTerminalBackendTarget();
	if (target.kind === "remote") {
		return remoteTerminalRequest<{ ok: boolean }>(
			target.environment,
			`/api/v1/terminals/${encodeURIComponent(ptyId)}/write`,
			{
				method: "POST",
				body: JSON.stringify({ data }),
			},
		);
	}
	return invoke<{ ok: boolean }>("terminal_write", { ptyId, data });
}

export function resizeTerminal(ptyId: string, cols: number, rows: number) {
	const target = getTerminalBackendTarget();
	if (target.kind === "remote") {
		return remoteTerminalRequest<{ ok: boolean }>(
			target.environment,
			`/api/v1/terminals/${encodeURIComponent(ptyId)}/resize`,
			{
				method: "POST",
				body: JSON.stringify({ cols, rows }),
			},
		);
	}
	return invoke<{ ok: boolean }>("terminal_resize", { ptyId, cols, rows });
}

export function killTerminal(ptyId: string) {
	const target = getTerminalBackendTarget();
	if (target.kind === "remote") {
		return remoteTerminalRequest<{ ok: boolean }>(
			target.environment,
			`/api/v1/terminals/${encodeURIComponent(ptyId)}/kill`,
			{
				method: "POST",
				body: JSON.stringify({}),
			},
		);
	}
	return invoke<{ ok: boolean }>("terminal_kill", { ptyId });
}

export async function listenTerminalOutput(
	handler: (event: TerminalOutputEvent) => void,
): Promise<UnlistenFn> {
	const target = getTerminalBackendTarget();
	if (target.kind === "remote") {
		await ensureRemoteTerminalBridge(target.environment);
		remoteOutputListeners.add(handler);
		return () => {
			remoteOutputListeners.delete(handler);
		};
	}

	return listen<TerminalOutputEvent>(TERMINAL_OUTPUT_EVENT, (event) => {
		handler(event.payload);
	});
}

export async function listenTerminalExit(
	handler: (event: TerminalExitEvent) => void,
): Promise<UnlistenFn> {
	const target = getTerminalBackendTarget();
	if (target.kind === "remote") {
		await ensureRemoteTerminalBridge(target.environment);
		remoteExitListeners.add(handler);
		return () => {
			remoteExitListeners.delete(handler);
		};
	}

	return listen<TerminalExitEvent>(TERMINAL_EXIT_EVENT, (event) => {
		handler(event.payload);
	});
}
