import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { openEventStream } from "./sseClient";
const session = {
	backendUrl: "http://host",
	deviceId: "device",
	sessionToken: "test-token",
	createdAt: "now",
};
let stop: (() => void) | undefined;
beforeEach(() => {
	vi.useFakeTimers();
	Object.defineProperty(navigator, "onLine", {
		configurable: true,
		value: true,
	});
});
afterEach(() => {
	stop?.();
	stop = undefined;
	vi.useRealTimers();
	vi.unstubAllGlobals();
});
function streamingFetch() {
	let stream: ReadableStreamDefaultController<Uint8Array>;
	const mock = vi.fn((_url: string, init: RequestInit) =>
		Promise.resolve(
			new Response(
				new ReadableStream<Uint8Array>({
					start(c) {
						stream = c;
						init.signal?.addEventListener(
							"abort",
							() => c.error(new DOMException("Aborted", "AbortError")),
							{ once: true },
						);
					},
				}),
			),
		),
	);
	vi.stubGlobal("fetch", mock);
	return {
		mock,
		send: (text: string) => stream.enqueue(new TextEncoder().encode(text)),
	};
}
describe("mobile event recovery", () => {
	it("decodes split frames, ignores keepalives, and reconciles on connection", async () => {
		const { send, mock } = streamingFetch();
		const onOpen = vi.fn();
		const onMessage = vi.fn();
		stop = openEventStream(session, "/events", { onOpen, onMessage });
		await vi.advanceTimersByTimeAsync(1);
		send(': keep-alive\r\n\r\ndata: {"ok":');
		await vi.advanceTimersByTimeAsync(1);
		send("true}\r\n\r\n");
		await vi.advanceTimersByTimeAsync(1);
		expect(onMessage).toHaveBeenCalledWith({ ok: true });
		expect(onOpen).toHaveBeenCalledTimes(1);
		expect(mock.mock.calls[0]![1].headers).toMatchObject({
			Authorization: "Bearer test-token",
		});
	});
	it("aborts a silent stream and reconnects after the heartbeat deadline", async () => {
		const { mock } = streamingFetch();
		const onOpen = vi.fn();
		stop = openEventStream(session, "/events", { onOpen, onMessage: vi.fn() });
		await vi.advanceTimersByTimeAsync(47_500);
		expect(mock.mock.calls.length).toBe(2);
		expect(onOpen).toHaveBeenCalledTimes(2);
	});
	it("does not keep retrying revoked credentials", async () => {
		const mock = vi.fn().mockResolvedValue(new Response("", { status: 401 }));
		vi.stubGlobal("fetch", mock);
		const onState = vi.fn();
		stop = openEventStream(session, "/events", { onMessage: vi.fn(), onState });
		await vi.advanceTimersByTimeAsync(120_000);
		expect(mock).toHaveBeenCalledTimes(1);
		expect(onState).toHaveBeenLastCalledWith("unauthorized");
	});
	it("reconnects on return to the foreground and cancels retries on unmount", async () => {
		const { mock } = streamingFetch();
		stop = openEventStream(session, "/events", { onMessage: vi.fn() });
		await vi.advanceTimersByTimeAsync(1);
		Object.defineProperty(document, "visibilityState", {
			configurable: true,
			value: "visible",
		});
		document.dispatchEvent(new Event("visibilitychange"));
		await vi.advanceTimersByTimeAsync(2_100);
		expect(mock).toHaveBeenCalledTimes(2);
		stop();
		await vi.advanceTimersByTimeAsync(120_000);
		expect(mock).toHaveBeenCalledTimes(2);
	});
	it("waits offline and resumes when the device comes online", async () => {
		Object.defineProperty(navigator, "onLine", {
			configurable: true,
			value: false,
		});
		const { mock } = streamingFetch();
		stop = openEventStream(session, "/events", { onMessage: vi.fn() });
		await vi.advanceTimersByTimeAsync(20_000);
		expect(mock).not.toHaveBeenCalled();
		Object.defineProperty(navigator, "onLine", {
			configurable: true,
			value: true,
		});
		window.dispatchEvent(new Event("online"));
		await vi.advanceTimersByTimeAsync(1);
		expect(mock).toHaveBeenCalledTimes(1);
	});
});
