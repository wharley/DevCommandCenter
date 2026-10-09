import type { RawSessionEvent } from "@dcc/mobile-core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Backend, StreamHandlers } from "../src/backend/types";
import { ThreadSync, type ThreadSnapshot } from "../src/screens/threadSync";

function ev(sequence: number, type: string, extra: Record<string, unknown> = {}): RawSessionEvent {
	return { eventId: `e${sequence}`, sessionId: "s", sequence, occurredAt: "2026-10-09T00:00:00Z", kind: { type, turnId: "t", ...extra } };
}

function fakeBackend(history: RawSessionEvent[]) {
	const requests: string[] = [];
	let stream: { path: () => string; handlers: StreamHandlers } | null = null;
	const backend: Backend = {
		mode: "remote",
		async request<T>(_method: string, path: string): Promise<T> {
			requests.push(path);
			const after = /after=(\d+)/.exec(path);
			if (!after) return history as T;
			const cursor = Number(after[1]);
			return { events: history.filter((e) => e.sequence > cursor), lastSeq: history.at(-1)?.sequence ?? 0, hasMore: false } as T;
		},
		subscribe(path, handlers) {
			stream = { path, handlers };
			return () => (stream = null);
		},
		state: () => ({ kind: "online" }),
		onState: () => () => {},
		reconnectNow() {},
		dispose() {},
	};
	return { backend, requests, stream: () => stream! };
}

describe("ThreadSync", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("subscribes after the history with its cursor and applies replayed events directly", async () => {
		const history = [ev(1, "turn_started", { prompt: "oi" }), ev(2, "turn_delta", { content: "olá" })];
		const { backend, requests, stream } = fakeBackend(history);
		let snapshot: ThreadSnapshot | null = null;
		const sync = new ThreadSync(backend, "s", (next) => (snapshot = next));
		sync.start();
		await vi.advanceTimersByTimeAsync(0);
		expect(stream().path()).toBe("/api/v1/events/stream?sessionId=s&after=2");

		stream().handlers.onEvent({ event: "replay", data: ev(3, "turn_delta", { content: ", tudo bem?" }), seq: 3 });
		stream().handlers.onEvent({ event: "replay", data: ev(2, "turn_delta", { content: "dup" }), seq: 2 });
		const answer = snapshot!.state.messages.find((m) => m.kind === "assistant");
		expect(answer?.kind === "assistant" && answer.text).toBe("olá, tudo bem?");
		// The next (re)open resumes after what the replay brought.
		expect(stream().path()).toBe("/api/v1/events/stream?sessionId=s&after=3");
		expect(requests).toHaveLength(1);
		sync.stop();
	});

	it("turns live events into one debounced delta and a rewind into a full refetch", async () => {
		const history = [ev(1, "turn_started", { prompt: "oi" })];
		const { backend, requests, stream } = fakeBackend(history);
		const sync = new ThreadSync(backend, "s", () => {});
		sync.start();
		await vi.advanceTimersByTimeAsync(0);
		history.push(ev(2, "turn_delta", { content: "a" }));
		const live = { event: "core-event", data: { sessionTurnDelta: { session_id: "s" } }, seq: 2 };
		stream().handlers.onEvent(live);
		stream().handlers.onEvent(live);
		stream().handlers.onEvent({ event: "core-event", data: { sessionTurnDelta: { session_id: "other" } } });
		await vi.advanceTimersByTimeAsync(200);
		expect(requests).toEqual(["/api/v1/sessions/s/events", "/api/v1/sessions/s/events?after=1&limit=500"]);

		stream().handlers.onEvent({ event: "replay", data: ev(3, "conversation_rewound"), seq: 3 });
		await vi.advanceTimersByTimeAsync(200);
		expect(requests.at(-1)).toBe("/api/v1/sessions/s/events");
		sync.stop();
	});
});
