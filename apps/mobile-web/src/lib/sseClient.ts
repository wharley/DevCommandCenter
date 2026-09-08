import type { PairingSession } from "./session";

export type StreamState =
	"connecting" | "connected" | "reconnecting" | "offline" | "unauthorized";
export type SseOptions = {
	signal?: AbortSignal;
	onMessage: (payload: unknown) => void;
	/** Reconcile durable history on every connection, including the first. */
	onOpen?: () => void;
	onError?: (error: unknown) => void;
	onState?: (state: StreamState) => void;
};

/** Fetch supports bearer headers; EventSource does not. Never retry mutations here. */
export function openEventStream(
	session: PairingSession,
	path: string,
	options: SseOptions,
): () => void {
	let closed = false;
	let attempt: AbortController | undefined;
	let wake: (() => void) | undefined;
	let delay = 1_500;
	let connectedBefore = false;
	const restart = () => {
		delay = 1_500;
		attempt?.abort();
		wake?.();
	};
	const visible = () => {
		if (document.visibilityState === "visible") restart();
	};
	const stop = () => {
		closed = true;
		attempt?.abort();
		wake?.();
		window.removeEventListener("online", restart);
		window.removeEventListener("offline", restart);
		document.removeEventListener("visibilitychange", visible);
		options.signal?.removeEventListener("abort", stop);
	};
	const pause = (ms: number) =>
		new Promise<void>((resolve) => {
			const timer = window.setTimeout(finish, ms);
			function finish() {
				clearTimeout(timer);
				wake = undefined;
				resolve();
			}
			wake = finish;
		});
	if (options.signal?.aborted) return stop;
	options.signal?.addEventListener("abort", stop, { once: true });
	window.addEventListener("online", restart);
	window.addEventListener("offline", restart);
	document.addEventListener("visibilitychange", visible);

	void (async () => {
		while (!closed) {
			if (!navigator.onLine) {
				options.onState?.("offline");
				await pause(15_000);
				continue;
			}
			attempt = new AbortController();
			let watchdog: ReturnType<typeof setTimeout>;
			// Server keepalives arrive every 15s, including while an agent is idle.
			const heartbeat = () => {
				clearTimeout(watchdog);
				watchdog = setTimeout(() => attempt?.abort(), 45_000);
			};
			options.onState?.(connectedBefore ? "reconnecting" : "connecting");
			heartbeat();
			try {
				const res = await fetch(`${session.backendUrl}${path}`, {
					headers: {
						Authorization: `Bearer ${session.sessionToken}`,
						Accept: "text/event-stream",
					},
					cache: "no-store",
					signal: attempt.signal,
				});
				if (res.status === 401 || res.status === 403) {
					options.onState?.("unauthorized");
					break;
				}
				if (!res.ok || !res.body) throw new Error(`SSE HTTP ${res.status}`);
				connectedBefore = true;
				delay = 1_500;
				options.onState?.("connected");
				options.onOpen?.();
				const reader = res.body.getReader();
				const decoder = new TextDecoder();
				let buffer = "";
				try {
					while (!closed) {
						const { value, done } = await reader.read();
						if (done) break;
						heartbeat();
						buffer += decoder.decode(value, { stream: true });
						let match: RegExpExecArray | null;
						while ((match = /\r?\n\r?\n/u.exec(buffer))) {
							const frame = buffer.slice(0, match.index);
							buffer = buffer.slice(match.index + match[0].length);
							const data = frame
								.split(/\r?\n/u)
								.filter((l) => l.startsWith("data:"))
								.map((l) => l.slice(5).trimStart())
								.join("\n");
							if (!data) continue;
							let payload: unknown;
							try {
								payload = JSON.parse(data);
							} catch {
								continue;
							}
							options.onMessage(payload);
						}
						if (buffer.length > 2_000_000)
							throw new Error("Evento excedeu o limite de tamanho.");
					}
				} finally {
					void reader.cancel().catch(() => {});
				}
			} catch (error) {
				if (!closed) options.onError?.(error);
			} finally {
				clearTimeout(watchdog!);
			}
			if (closed) break;
			options.onState?.(navigator.onLine ? "reconnecting" : "offline");
			await pause(delay + Math.random() * 500);
			delay = Math.min(delay * 2, 15_000);
		}
	})();
	return stop;
}
