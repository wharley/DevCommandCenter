/**
 * What the screens talk to. Two implementations: the encrypted relay channel
 * to the paired desktop (`remote.ts`) and the offline demo (`demo.ts`), so the
 * demo exercises exactly the screens a paired phone uses.
 */

export type ConnectionState =
	| { kind: "connecting"; attempt: number }
	| { kind: "online" }
	| { kind: "offline"; error: string; retryAt: number }
	/** The desktop no longer knows this phone (revoked or forgotten). */
	| { kind: "unpaired"; error: string };

export type StreamEvent = {
	/** SSE event name: `core-event`, `replay` (history after the cursor), `resync`. */
	event: string;
	data: unknown;
	seq?: number;
};

export type StreamHandlers = {
	onEvent: (event: StreamEvent) => void;
	/** The stream (re)opened; `reopened` is true after a reconnect. */
	onOpen?: (reopened: boolean) => void;
};

export interface Backend {
	readonly mode: "remote" | "demo";
	request<T = unknown>(method: string, path: string, body?: unknown): Promise<T>;
	/**
	 * Keeps an SSE path of the desktop API open across reconnects. `path` is
	 * read on every (re)open so a thread can resume from its latest cursor.
	 */
	subscribe(path: () => string, handlers: StreamHandlers): () => void;
	state(): ConnectionState;
	onState(listener: (state: ConnectionState) => void): () => void;
	/** Skip the backoff (pull-to-refresh, app back in the foreground). */
	reconnectNow(): void;
	dispose(): void;
}

export class ApiError extends Error {
	readonly status: number;
	readonly body: unknown;

	constructor(status: number, message: string, body: unknown = null) {
		super(message);
		this.name = "ApiError";
		this.status = status;
		this.body = body;
	}
}

/** `409 already_resolved`: someone (the desktop, another phone) answered first. */
export function isAlreadyResolved(error: unknown): boolean {
	return error instanceof ApiError && error.status === 409;
}

export function errorMessage(error: unknown, fallback = "Algo deu errado."): string {
	return error instanceof Error && error.message ? error.message : fallback;
}
