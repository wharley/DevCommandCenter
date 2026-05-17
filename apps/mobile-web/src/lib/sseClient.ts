import type { PairingSession } from "./session";

/**
 * EventSource cannot carry an `Authorization` header, which we need for the
 * bearer-token mobile flow. So we open the stream manually with `fetch`,
 * parse the SSE frame format off the response body, and reconnect on failure.
 */

export type SseOptions = {
	signal?: AbortSignal;
	/** Called once per JSON-decoded `data:` block. */
	onMessage: (payload: unknown) => void;
	/** Called when the stream goes down (will be followed by reconnect). */
	onError?: (error: unknown) => void;
};

const RECONNECT_DELAY_MS = 1_500;
const MAX_RECONNECT_DELAY_MS = 15_000;

export function openEventStream(
	session: PairingSession,
	path: string,
	options: SseOptions,
): () => void {
	const controller = new AbortController();
	const onAbort = () => controller.abort();
	options.signal?.addEventListener("abort", onAbort);
	let closed = false;
	let delay = RECONNECT_DELAY_MS;

	const url = `${session.backendUrl}${path.startsWith("/") ? path : `/${path}`}`;

	void (async () => {
		while (!closed) {
			try {
				const res = await fetch(url, {
					method: "GET",
					headers: {
						Authorization: `Bearer ${session.sessionToken}`,
						Accept: "text/event-stream",
					},
					signal: controller.signal,
				});
				if (!res.ok || !res.body) {
					throw new Error(`SSE returned HTTP ${res.status}`);
				}
				delay = RECONNECT_DELAY_MS;
				const reader = res.body.getReader();
				const decoder = new TextDecoder();
				let buffer = "";
				try {
					while (!closed) {
						const { value, done } = await reader.read();
						if (done) break;
						buffer += decoder.decode(value, { stream: true });
						let boundary = buffer.search(/\r?\n\r?\n/u);
						while (boundary >= 0) {
							const sepMatch = buffer.slice(boundary).match(/^\r?\n\r?\n/u);
							const sepLen = sepMatch?.[0]?.length ?? 2;
							const frame = buffer.slice(0, boundary);
							buffer = buffer.slice(boundary + sepLen);
							const data = frame
								.split(/\r?\n/u)
								.filter((line) => line.startsWith("data:"))
								.map((line) => line.slice(5).trimStart())
								.join("\n");
							if (data.length > 0) {
								try {
									options.onMessage(JSON.parse(data));
								} catch {
									/* ignore non-JSON SSE frames (keepalives etc.) */
								}
							}
							boundary = buffer.search(/\r?\n\r?\n/u);
						}
					}
				} finally {
					void reader.cancel().catch(() => {});
				}
			} catch (err) {
				if (!closed && !controller.signal.aborted) {
					options.onError?.(err);
				}
			}
			if (closed || controller.signal.aborted) break;
			await new Promise((r) => window.setTimeout(r, delay));
			delay = Math.min(delay * 2, MAX_RECONNECT_DELAY_MS);
		}
	})();

	return () => {
		closed = true;
		controller.abort();
		options.signal?.removeEventListener("abort", onAbort);
	};
}
