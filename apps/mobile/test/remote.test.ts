import { utf8 } from "@dcc/remote-protocol";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RemoteBackend, type ChannelClient } from "../src/backend/remote";
import { ApiError, type ConnectionState, type StreamEvent } from "../src/backend/types";

type FakeClient = ChannelClient & {
	drop(code: number | null, message?: string): void;
	subscribed: string[];
	push(event: StreamEvent): void;
	responder: (method: string, path: string) => { status: number; body: unknown };
	pingOk: boolean;
};

function fakeClient(): FakeClient {
	const closeListeners = new Set<(error: { message: string; code: number | null }) => void>();
	const streams = new Set<(event: StreamEvent) => void>();
	let open = true;
	const client: FakeClient = {
		get isOpen() {
			return open;
		},
		subscribed: [],
		pingOk: true,
		responder: () => ({ status: 200, body: { ok: true } }),
		async request(method, path) {
			const { status, body } = client.responder(method, path);
			const bytes = utf8(JSON.stringify(body));
			return {
				status,
				headers: { "content-type": "application/json" },
				body: bytes,
				text: () => JSON.stringify(body),
				json: <T,>() => body as T,
			};
		},
		subscribe(path, handlers) {
			client.subscribed.push(path);
			streams.add(handlers.onEvent);
			return { cancel: () => streams.delete(handlers.onEvent) };
		},
		ping: () => (client.pingOk ? Promise.resolve() : new Promise(() => {})),
		close: () => client.drop(1000, "closed by the device"),
		onClose(listener) {
			closeListeners.add(listener);
			return () => closeListeners.delete(listener);
		},
		drop(code, message = "gone") {
			if (!open) return;
			open = false;
			streams.clear();
			for (const listener of [...closeListeners]) listener({ code, message });
		},
		push(event) {
			for (const listener of [...streams]) listener(event);
		},
	};
	return client;
}

describe("RemoteBackend", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("reconnects with backoff and reopens subscriptions with the latest path", async () => {
		const clients: FakeClient[] = [];
		let fail = 0;
		const backend = new RemoteBackend({
			random: () => 1,
			connect: async () => {
				if (fail > 0) {
					fail -= 1;
					throw Object.assign(new Error("relay down"), { code: null });
				}
				const client = fakeClient();
				clients.push(client);
				return client;
			},
		});
		const states: ConnectionState["kind"][] = [];
		backend.onState((state) => states.push(state.kind));
		await vi.advanceTimersByTimeAsync(0);
		expect(backend.state().kind).toBe("online");

		let cursor = 3;
		const opens: boolean[] = [];
		const events: StreamEvent[] = [];
		backend.subscribe(() => `/api/v1/events/stream?sessionId=s&after=${cursor}`, {
			onOpen: (reopened) => opens.push(reopened),
			onEvent: (event) => events.push(event),
		});
		expect(clients[0]!.subscribed).toEqual(["/api/v1/events/stream?sessionId=s&after=3"]);
		clients[0]!.push({ event: "core-event", data: {}, seq: 4 });
		expect(events).toHaveLength(1);

		cursor = 9;
		fail = 2;
		clients[0]!.drop(1006, "network lost");
		expect(backend.state()).toMatchObject({ kind: "offline", error: "network lost" });
		await vi.advanceTimersByTimeAsync(1_000); // attempt 1 fails
		await vi.advanceTimersByTimeAsync(2_000); // attempt 2 fails
		expect(clients).toHaveLength(1);
		await vi.advanceTimersByTimeAsync(4_000); // attempt 3 connects
		expect(clients).toHaveLength(2);
		expect(backend.state().kind).toBe("online");
		expect(clients[1]!.subscribed).toEqual(["/api/v1/events/stream?sessionId=s&after=9"]);
		expect(opens).toEqual([false, true]);
		expect(states).toContain("connecting");
		backend.dispose();
	});

	it("stops retrying once the desktop forgets the device", async () => {
		const client = fakeClient();
		let dials = 0;
		const backend = new RemoteBackend({
			initial: client,
			connect: async () => {
				dials += 1;
				throw Object.assign(new Error("this device is not paired with the desktop"), { code: 4401 });
			},
		});
		client.drop(4499, "this device was revoked on the desktop");
		expect(backend.state()).toMatchObject({ kind: "unpaired" });
		await vi.advanceTimersByTimeAsync(120_000);
		expect(dials).toBe(0);
		await expect(backend.request("GET", "/api/v1/combs")).rejects.toBeInstanceOf(ApiError);
		backend.dispose();
	});

	it("maps HTTP errors, keeping 409 recognisable", async () => {
		const client = fakeClient();
		client.responder = (_method, path) =>
			path.includes("respond-permission")
				? { status: 409, body: { ok: false, error: { code: "already_resolved", outcome: { kind: "permission" } } } }
				: { status: 200, body: [1, 2] };
		const backend = new RemoteBackend({ initial: client, connect: async () => fakeClient() });
		await expect(backend.request("GET", "/api/v1/combs")).resolves.toEqual([1, 2]);
		const error = await backend.request("POST", "/api/v1/sessions/s/respond-permission", {}).catch((e: unknown) => e);
		expect(error).toBeInstanceOf(ApiError);
		expect((error as ApiError).status).toBe(409);
		backend.dispose();
	});

	it("waits for the channel instead of failing a request while reconnecting", async () => {
		const clients: FakeClient[] = [];
		const backend = new RemoteBackend({
			random: () => 1,
			connect: async () => {
				const client = fakeClient();
				clients.push(client);
				return client;
			},
		});
		await vi.advanceTimersByTimeAsync(0);
		clients[0]!.drop(1006);
		const pending = backend.request<{ ok: boolean }>("GET", "/api/v1/status");
		await vi.advanceTimersByTimeAsync(0);
		expect(clients).toHaveLength(2); // reconnectNow skipped the backoff
		await expect(pending).resolves.toEqual({ ok: true });
		backend.dispose();
	});

	it("probes the socket when the app returns and redials a dead one", async () => {
		const clients: FakeClient[] = [];
		const backend = new RemoteBackend({
			pingTimeoutMs: 1_000,
			random: () => 1,
			connect: async () => {
				const client = fakeClient();
				clients.push(client);
				return client;
			},
		});
		await vi.advanceTimersByTimeAsync(0);
		clients[0]!.pingOk = false;
		const probe = backend.foreground();
		await vi.advanceTimersByTimeAsync(1_000);
		await probe;
		expect(clients[0]!.isOpen).toBe(false);
		await vi.advanceTimersByTimeAsync(1_000);
		expect(clients).toHaveLength(2);
		expect(backend.state().kind).toBe("online");
		backend.dispose();
	});
});

describe("RemoteBackend after dispose", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("does not schedule a retry when a dial fails after dispose", async () => {
		let dials = 0;
		let fail: (error: Error) => void = () => {};
		const backend = new RemoteBackend({
			connect: () => {
				dials += 1;
				return new Promise<ChannelClient>((_, reject) => (fail = reject));
			},
		});
		backend.dispose();
		fail(Object.assign(new Error("relay down"), { code: null }));
		await vi.advanceTimersByTimeAsync(120_000);
		expect(dials).toBe(1);
	});
});
