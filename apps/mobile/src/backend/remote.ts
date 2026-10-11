/**
 * The paired desktop over the relay: one encrypted channel (Noise IK) kept up
 * with exponential backoff, requests mapped to `ApiError`, and SSE
 * subscriptions that reopen by themselves after every reconnect.
 *
 * Foreground handling follows Paseo (`packages/app/src/runtime/host-runtime.ts`,
 * `setAppVisible`): while hidden the normal backoff keeps going for as long as
 * the OS lets JS run; coming back to the foreground skips the backoff and
 * probes an open socket with a ping instead of trusting it, since iOS kills
 * sockets of suspended apps without telling anyone.
 */
import { CloseCode, type RemoteResponse } from "@dcc/remote-protocol";
import {
	ApiError,
	type Backend,
	type ConnectionState,
	type StreamEvent,
	type StreamHandlers,
} from "./types";

/** The slice of `RemoteClient` this module uses (a fake in tests). */
export type ChannelClient = {
	readonly isOpen: boolean;
	request(method: string, path: string, init?: { body?: unknown }): Promise<RemoteResponse>;
	subscribe(
		path: string,
		handlers: { onEvent: (event: StreamEvent) => void; onEnd?: (error?: string) => void },
	): { cancel(): void };
	ping(): Promise<void>;
	close(): void;
	onClose(listener: (error: { message: string; code: number | null }) => void): () => void;
};

export type RemoteOptions = {
	connect: () => Promise<ChannelClient>;
	/** An already-open client (right after pairing) to adopt instead of dialing. */
	initial?: ChannelClient;
	backoff?: { baseMs: number; maxMs: number };
	requestTimeoutMs?: number;
	/** How long a request waits for the channel to come back before failing. */
	waitOnlineMs?: number;
	pingTimeoutMs?: number;
	random?: () => number;
	now?: () => number;
};

/** Closing with these means the desktop no longer accepts this phone. */
const UNPAIRED_CODES = new Set<number>([CloseCode.UNKNOWN_DEVICE, CloseCode.REVOKED]);
/** apps/relay: a phone connected to a room whose desktop is not online. */
const RELAY_NO_DESKTOP = 4603;

type Subscription = {
	path: () => string;
	handlers: StreamHandlers;
	opened: boolean;
	remote: { cancel(): void } | null;
	retry: ReturnType<typeof setTimeout> | null;
};

export class RemoteBackend implements Backend {
	readonly mode = "remote" as const;
	private readonly options: Required<Omit<RemoteOptions, "initial">>;
	private client: ChannelClient | null = null;
	private current: ConnectionState = { kind: "connecting", attempt: 0 };
	private readonly listeners = new Set<(state: ConnectionState) => void>();
	private readonly subs = new Set<Subscription>();
	private attempt = 0;
	private timer: ReturnType<typeof setTimeout> | null = null;
	private dialing = false;
	private disposed = false;
	private waiters: Array<() => void> = [];

	constructor(options: RemoteOptions) {
		this.options = {
			backoff: { baseMs: 1_000, maxMs: 30_000 },
			requestTimeoutMs: 30_000,
			waitOnlineMs: 12_000,
			pingTimeoutMs: 4_000,
			random: Math.random,
			now: Date.now,
			...options,
		};
		if (options.initial?.isOpen) this.attach(options.initial);
		else void this.dial();
	}

	state(): ConnectionState {
		return this.current;
	}

	onState(listener: (state: ConnectionState) => void): () => void {
		this.listeners.add(listener);
		return () => this.listeners.delete(listener);
	}

	reconnectNow(): void {
		if (this.disposed || this.current.kind === "unpaired") return;
		if (this.client?.isOpen) return;
		this.clearTimer();
		this.attempt = 0;
		void this.dial();
	}

	/** The app is visible again: verify the socket, or redial without waiting. */
	async foreground(): Promise<void> {
		const client = this.client;
		if (!client?.isOpen) {
			this.reconnectNow();
			return;
		}
		const alive = await Promise.race([
			client.ping().then(() => true),
			new Promise<boolean>((resolve) => setTimeout(() => resolve(false), this.options.pingTimeoutMs)),
		]).catch(() => false);
		if (!alive && this.client === client) client.close();
	}

	async request<T>(method: string, path: string, body?: unknown): Promise<T> {
		const client = await this.online();
		const response = await withTimeout(
			client.request(method, path, body === undefined ? {} : { body }),
			this.options.requestTimeoutMs,
			"O computador demorou para responder. Uma ação enviada pode ainda estar em andamento.",
		);
		return parseResponse<T>(response);
	}

	subscribe(path: () => string, handlers: StreamHandlers): () => void {
		const sub: Subscription = { path, handlers, opened: false, remote: null, retry: null };
		this.subs.add(sub);
		if (this.client?.isOpen) this.open(sub, this.client);
		return () => {
			this.subs.delete(sub);
			if (sub.retry) clearTimeout(sub.retry);
			sub.remote?.cancel();
			sub.remote = null;
		};
	}

	dispose(): void {
		if (this.disposed) return;
		this.disposed = true;
		this.clearTimer();
		for (const sub of this.subs) {
			if (sub.retry) clearTimeout(sub.retry);
			sub.remote = null;
		}
		this.subs.clear();
		this.client?.close();
		this.client = null;
		this.listeners.clear();
		this.releaseWaiters();
	}

	private online(): Promise<ChannelClient> {
		if (this.client?.isOpen) return Promise.resolve(this.client);
		if (this.current.kind === "unpaired") {
			return Promise.reject(new ApiError(401, this.current.error));
		}
		this.reconnectNow();
		return new Promise((resolve, reject) => {
			const timer = setTimeout(() => {
				this.waiters = this.waiters.filter((waiter) => waiter !== check);
				reject(new Error(offlineMessage(this.current)));
			}, this.options.waitOnlineMs);
			const check = () => {
				clearTimeout(timer);
				if (this.client?.isOpen) resolve(this.client);
				else reject(new Error(offlineMessage(this.current)));
			};
			this.waiters.push(check);
		});
	}

	private releaseWaiters() {
		const waiters = this.waiters;
		this.waiters = [];
		for (const waiter of waiters) waiter();
	}

	private async dial() {
		if (this.dialing || this.disposed) return;
		this.dialing = true;
		this.attempt += 1;
		this.setState({ kind: "connecting", attempt: this.attempt });
		try {
			const client = await this.options.connect();
			if (this.disposed) {
				client.close();
				return;
			}
			this.attach(client);
		} catch (error) {
			const code = (error as { code?: number | null }).code ?? null;
			this.lost(code, (error as Error).message);
		} finally {
			this.dialing = false;
		}
	}

	private attach(client: ChannelClient) {
		this.client = client;
		this.attempt = 0;
		client.onClose((error) => {
			if (this.client !== client) return;
			this.client = null;
			for (const sub of this.subs) {
				if (sub.retry) clearTimeout(sub.retry);
				sub.retry = null;
				sub.remote = null;
			}
			if (!this.disposed) this.lost(error.code, error.message);
		});
		this.setState({ kind: "online" });
		for (const sub of this.subs) this.open(sub, client);
		this.releaseWaiters();
	}

	private open(sub: Subscription, client: ChannelClient) {
		const reopened = sub.opened;
		sub.opened = true;
		sub.remote = client.subscribe(sub.path(), {
			onEvent: (event) => sub.handlers.onEvent(event),
			onEnd: () => {
				sub.remote = null;
				// The desktop ended the stream while the channel stays up
				// (its SSE restarted): open it again shortly.
				if (!this.subs.has(sub) || this.client !== client || !client.isOpen) return;
				sub.retry = setTimeout(() => {
					sub.retry = null;
					if (this.subs.has(sub) && this.client === client && client.isOpen) this.open(sub, client);
				}, 1_000);
			},
		});
		sub.handlers.onOpen?.(reopened);
	}

	private lost(code: number | null, message: string) {
		if (this.disposed) return;
		const error = connectionErrorMessage(code, message);
		if (code !== null && UNPAIRED_CODES.has(code)) {
			this.setState({ kind: "unpaired", error });
			this.releaseWaiters();
			return;
		}
		const { baseMs, maxMs } = this.options.backoff;
		const exp = Math.min(maxMs, baseMs * 2 ** Math.max(0, this.attempt - 1));
		const delay = Math.round(exp / 2 + this.options.random() * (exp / 2));
		this.setState({ kind: "offline", error, retryAt: this.options.now() + delay });
		this.releaseWaiters();
		this.clearTimer();
		this.timer = setTimeout(() => {
			this.timer = null;
			void this.dial();
		}, delay);
	}

	private clearTimer() {
		if (this.timer) clearTimeout(this.timer);
		this.timer = null;
	}

	private setState(state: ConnectionState) {
		this.current = state;
		for (const listener of [...this.listeners]) listener(state);
	}
}

/**
 * The channel's errors come in English from `@dcc/remote-protocol` and the
 * platform's WebSocket; the banner says what happened in Portuguese.
 */
export function connectionErrorMessage(code: number | null, message: string): string {
	switch (code) {
		case RELAY_NO_DESKTOP:
			return "O computador está offline ou com o DCC fechado.";
		case CloseCode.UNKNOWN_DEVICE:
			return "Este aparelho não está mais pareado com o computador. Pareie de novo.";
		case CloseCode.REVOKED:
			return "Este aparelho foi removido no computador. Pareie de novo.";
		case CloseCode.PROTOCOL:
			return "O computador recusou a conexão (versão incompatível).";
		default:
			break;
	}
	if (/timed out connecting/.test(message)) return "O computador demorou para responder pelo relay.";
	return code !== null && code !== 1000 ? `Sem conexão com o relay (código ${code}).` : "Sem conexão com o relay.";
}

function offlineMessage(state: ConnectionState): string {
	if (state.kind === "offline") return `Sem conexão com o computador: ${state.error}`;
	if (state.kind === "unpaired") return state.error;
	return "Sem conexão com o computador.";
}

function withTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
	return new Promise((resolve, reject) => {
		const timer = setTimeout(() => reject(new Error(message)), ms);
		promise.then(
			(value) => {
				clearTimeout(timer);
				resolve(value);
			},
			(error: unknown) => {
				clearTimeout(timer);
				reject(error);
			},
		);
	});
}

export function parseResponse<T>(response: RemoteResponse): T {
	const contentType = (response.headers["content-type"] ?? "").toLowerCase();
	let body: unknown = null;
	if (response.body.length > 0) {
		const text = response.text();
		body = contentType.includes("json") || /^[[{]/.test(text.trim()) ? safeJson(text) : text;
	}
	if (response.status >= 400) {
		throw new ApiError(response.status, errorText(response.status, body), body);
	}
	return (response.status === 204 ? undefined : body) as T;
}

function safeJson(text: string): unknown {
	try {
		return JSON.parse(text);
	} catch {
		return text;
	}
}

function errorText(status: number, body: unknown): string {
	const error = (body as { error?: unknown } | null)?.error;
	if (typeof error === "string" && error) return error;
	if (error && typeof (error as { message?: unknown }).message === "string") {
		return (error as { message: string }).message;
	}
	if (status === 409) return "Já foi respondido em outro lugar.";
	if (status === 401 || status === 403) return "O computador recusou este aparelho. Pareie de novo.";
	return `HTTP ${status}`;
}
