import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionState } from "../src/backend/types";
import {
	PRESENCE_PATH,
	PUSH_TOKEN_PATH,
	PushRegistration,
	reportPresence,
	sessionFromNotificationData,
	type AppVisibility,
	type PermissionInfo,
	type PushChannel,
	type PushPlatform,
	type PushPreference,
} from "../src/notifications/registration";

type Call = { method: string; path: string; body?: unknown };

function fakeChannel(initial: ConnectionState = { kind: "online" }) {
	let state = initial;
	const listeners = new Set<(state: ConnectionState) => void>();
	const calls: Call[] = [];
	const channel: PushChannel & { calls: Call[]; go(state: ConnectionState): void; fail: boolean } = {
		calls,
		fail: false,
		async request<T>(method: string, path: string, body?: unknown) {
			calls.push({ method, path, body });
			if (channel.fail) throw new Error("O computador demorou para responder.");
			if (method === "PUT" && path === PUSH_TOKEN_PATH) {
				return { ok: true, expiresAt: "2026-10-11T12:00:00.000Z" } as T;
			}
			return { ok: true } as T;
		},
		state: () => state,
		onState(listener) {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		go(next) {
			state = next;
			for (const listener of [...listeners]) listener(next);
		},
	};
	return channel;
}

function fakePlatform(permission: PermissionInfo, token: string | null = "ExponentPushToken[phone]") {
	const platform: PushPlatform & {
		permission: PermissionInfo;
		answer: PermissionInfo;
		token: string | null;
		asked: number;
		openedSettings: number;
		prepared: number;
	} = {
		os: "ios",
		permission,
		answer: { status: "granted", canAskAgain: true },
		token,
		asked: 0,
		openedSettings: 0,
		prepared: 0,
		getPermission: async () => platform.permission,
		async requestPermission() {
			platform.asked += 1;
			platform.permission = platform.answer;
			return platform.answer;
		},
		async prepare() {
			platform.prepared += 1;
		},
		getToken: async () => platform.token,
		async openSettings() {
			platform.openedSettings += 1;
		},
	};
	return platform;
}

function memoryPreferences(initial: PushPreference = "unset") {
	const store = {
		value: initial,
		load: async () => store.value,
		save: async (preference: PushPreference) => {
			store.value = preference;
		},
	};
	return store;
}

const undetermined: PermissionInfo = { status: "undetermined", canAskAgain: true };
const granted: PermissionInfo = { status: "granted", canAskAgain: true };

const tokenCalls = (calls: Call[]) => calls.filter((call) => call.path === PUSH_TOKEN_PATH);

describe("PushRegistration", () => {
	it("does nothing (and asks nothing) until the person says yes", async () => {
		const channel = fakeChannel();
		const platform = fakePlatform(undetermined);
		const push = new PushRegistration(channel, platform, memoryPreferences());
		push.start();
		await vi.waitFor(() => expect(push.status().kind).toBe("unset"));
		expect(platform.asked).toBe(0);
		expect(channel.calls).toEqual([]);
		expect(await push.shouldOffer()).toBe(true);
	});

	it("asks the OS on yes, then sends the token over the channel", async () => {
		const channel = fakeChannel();
		const platform = fakePlatform(undetermined);
		const prefs = memoryPreferences();
		const push = new PushRegistration(channel, platform, prefs);
		const status = await push.enable();
		expect(platform.asked).toBe(1);
		expect(platform.prepared).toBe(1);
		expect(status).toEqual({ kind: "on", expiresAt: "2026-10-11T12:00:00.000Z" });
		expect(prefs.value).toBe("on");
		expect(tokenCalls(channel.calls)).toEqual([
			{ method: "PUT", path: PUSH_TOKEN_PATH, body: { token: "ExponentPushToken[phone]", platform: "ios" } },
		]);
		expect(await push.shouldOffer()).toBe(false);
	});

	it("renews the lease on every connection", async () => {
		const channel = fakeChannel({ kind: "connecting", attempt: 1 });
		const push = new PushRegistration(channel, fakePlatform(granted), memoryPreferences("on"));
		push.start();
		await vi.waitFor(() => expect(push.status().kind).toBe("pending"));
		expect(tokenCalls(channel.calls)).toEqual([]);

		channel.go({ kind: "online" });
		await vi.waitFor(() => expect(tokenCalls(channel.calls)).toHaveLength(1));
		channel.go({ kind: "offline", error: "x", retryAt: 0 });
		channel.go({ kind: "online" });
		await vi.waitFor(() => expect(tokenCalls(channel.calls)).toHaveLength(2));
		expect(push.status().kind).toBe("on");
	});

	it("turning it off removes the token from the desktop and stays off", async () => {
		const channel = fakeChannel();
		const prefs = memoryPreferences("on");
		const push = new PushRegistration(channel, fakePlatform(granted), prefs);
		await push.sync();
		const status = await push.disable();
		expect(status.kind).toBe("off");
		expect(prefs.value).toBe("off");
		expect(tokenCalls(channel.calls).at(-1)).toEqual({ method: "DELETE", path: PUSH_TOKEN_PATH, body: undefined });

		channel.go({ kind: "online" });
		await push.sync();
		expect(tokenCalls(channel.calls).filter((call) => call.method === "PUT")).toHaveLength(1);
		expect(await push.shouldOffer()).toBe(false);
	});

	it("a no at the system prompt is remembered", async () => {
		const platform = fakePlatform(undetermined);
		platform.answer = { status: "denied", canAskAgain: false };
		const prefs = memoryPreferences();
		const push = new PushRegistration(fakeChannel(), platform, prefs);
		expect(await push.enable()).toEqual({ kind: "denied", canAskAgain: false });
		expect(prefs.value).toBe("off");
	});

	it("a permission that cannot be asked again opens the system settings", async () => {
		const platform = fakePlatform({ status: "denied", canAskAgain: false });
		const push = new PushRegistration(fakeChannel(), platform, memoryPreferences("off"));
		expect((await push.enable()).kind).toBe("denied");
		expect(platform.openedSettings).toBe(1);
		expect(platform.asked).toBe(0);
	});

	it("permission revoked in the system settings removes the token", async () => {
		const channel = fakeChannel();
		const platform = fakePlatform(granted);
		const push = new PushRegistration(channel, platform, memoryPreferences("on"));
		await push.sync();
		platform.permission = { status: "denied", canAskAgain: false };
		expect((await push.sync()).kind).toBe("denied");
		expect(tokenCalls(channel.calls).map((call) => call.method)).toEqual(["PUT", "DELETE"]);
	});

	it("without an EAS projectId there is no token to send", async () => {
		const channel = fakeChannel();
		const push = new PushRegistration(channel, fakePlatform(granted, null), memoryPreferences("on"));
		expect(await push.sync()).toEqual({ kind: "unavailable" });
		expect(channel.calls).toEqual([]);
	});

	it("a failed registration is reported and retried on the next connection", async () => {
		const channel = fakeChannel();
		channel.fail = true;
		const push = new PushRegistration(channel, fakePlatform(granted), memoryPreferences("on"));
		push.start();
		await vi.waitFor(() => expect(push.status().kind).toBe("error"));
		channel.fail = false;
		channel.go({ kind: "online" });
		await vi.waitFor(() => expect(push.status().kind).toBe("on"));
	});

	it("overlapping syncs fold into one more run", async () => {
		const channel = fakeChannel();
		const push = new PushRegistration(channel, fakePlatform(granted), memoryPreferences("on"));
		await Promise.all([push.sync(), push.sync(), push.sync()]);
		expect(tokenCalls(channel.calls)).toHaveLength(2);
	});

	it("the web build has no push", async () => {
		const platform = { ...fakePlatform(granted), os: "web" as const };
		const push = new PushRegistration(fakeChannel(), platform, memoryPreferences("on"));
		expect(await push.shouldOffer()).toBe(false);
		expect((await push.sync()).kind).toBe("unsupported");
	});
});

describe("reportPresence", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	function visibility(initial: AppVisibility) {
		let current = initial;
		const listeners = new Set<(state: AppVisibility) => void>();
		return {
			current: () => current,
			onChange(listener: (state: AppVisibility) => void) {
				listeners.add(listener);
				return () => listeners.delete(listener);
			},
			set(next: AppVisibility) {
				current = next;
				for (const listener of [...listeners]) listener(next);
			},
		};
	}

	const states = (calls: Call[]) =>
		calls.filter((call) => call.path === PRESENCE_PATH).map((call) => (call.body as { state: string }).state);

	it("says so on connect, on every change and every 30 s while visible", () => {
		const channel = fakeChannel();
		const app = visibility("active");
		const stop = reportPresence(channel, app, 30_000);
		expect(states(channel.calls)).toEqual(["active"]);
		vi.advanceTimersByTime(60_000);
		expect(states(channel.calls)).toEqual(["active", "active", "active"]);

		app.set("inactive");
		expect(states(channel.calls).at(-1)).toBe("background");
		vi.advanceTimersByTime(120_000);
		expect(states(channel.calls)).toHaveLength(4);

		app.set("active");
		channel.go({ kind: "online" });
		expect(states(channel.calls).slice(-2)).toEqual(["active", "active"]);
		stop();
		vi.advanceTimersByTime(120_000);
		expect(states(channel.calls)).toHaveLength(6);
	});

	it("stays quiet while offline", () => {
		const channel = fakeChannel({ kind: "connecting", attempt: 1 });
		reportPresence(channel, visibility("active"), 30_000);
		vi.advanceTimersByTime(90_000);
		expect(channel.calls).toEqual([]);
	});
});

describe("sessionFromNotificationData", () => {
	it("reads the session id the desktop puts in the payload", () => {
		expect(sessionFromNotificationData({ sessionId: " abc ", kind: "permission" })).toBe("abc");
		expect(sessionFromNotificationData('{"sessionId":"abc"}')).toBe("abc");
		for (const bad of [null, undefined, "", "{", [], { sessionId: 1 }, { sessionId: "  " }, { other: "x" }]) {
			expect(sessionFromNotificationData(bad)).toBeNull();
		}
	});
});
