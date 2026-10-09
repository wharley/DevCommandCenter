/**
 * Push notifications for this phone: the Expo push token goes to the paired
 * desktop only through the encrypted channel (`PUT /api/v1/remote/push-token`,
 * answered by the desktop's relay connector, never by Expo or the relay).
 *
 * - The desktop keeps a token for 48 h; registering again on every connection
 *   renews it (Paseo, `push-notifications/internal/subscriptions.ts`:
 *   `subscribeConnectionStatus` → `registerPushToken` on each `connected`).
 * - The OS permission is asked only when the person says yes on our own
 *   explanation screen right after pairing, or turns it on in Settings —
 *   never on launch. A denied permission that cannot be asked again sends
 *   them to the system settings (Happy, `pushRegistration.ts`,
 *   `requestPushPermissionOrOpenSettings`).
 * - Turning it off here removes the token from the desktop
 *   (`DELETE /api/v1/remote/push-token`; Paseo's `revokeSubscription`).
 * - Presence: the app tells the desktop whether it is in the foreground, so
 *   a phone that is looking at the app gets no push (Paseo's `appVisible`,
 *   Happy's `app-state`). Repeated every 30 s while visible; the desktop
 *   forgets it after 75 s without news, so a suspended app gets pushes.
 *
 * Pure TypeScript with the platform injected (`native.ts` wires Expo), so the
 * tests drive it with fakes.
 */
import type { ConnectionState } from "../backend/types";

export type PermissionInfo = {
	status: "granted" | "denied" | "undetermined";
	canAskAgain: boolean;
};

export type PushPlatform = {
	readonly os: "ios" | "android" | "web";
	getPermission(): Promise<PermissionInfo>;
	requestPermission(): Promise<PermissionInfo>;
	/** Creates the Android channel the desktop's pushes name. */
	prepare(): Promise<void>;
	/** The Expo push token, or null when this build cannot have one (no EAS projectId). */
	getToken(): Promise<string | null>;
	openSettings(): Promise<void>;
};

/** `unset`: the person was never asked (the explanation screen shows once). */
export type PushPreference = "unset" | "on" | "off";

export type PreferenceStore = {
	load(): Promise<PushPreference>;
	save(preference: PushPreference): Promise<void>;
};

export type PushStatus =
	| { kind: "unsupported" }
	| { kind: "unset" }
	| { kind: "off" }
	| { kind: "denied"; canAskAgain: boolean }
	/** This build has no EAS projectId, so Expo cannot issue a token. */
	| { kind: "unavailable" }
	/** On, but the desktop has not been told yet (offline). */
	| { kind: "pending" }
	| { kind: "on"; expiresAt: string | null }
	| { kind: "error"; message: string };

/** The slice of the remote backend this module needs. */
export type PushChannel = {
	request<T = unknown>(method: string, path: string, body?: unknown): Promise<T>;
	state(): ConnectionState;
	onState(listener: (state: ConnectionState) => void): () => void;
};

export const PUSH_TOKEN_PATH = "/api/v1/remote/push-token";
export const PRESENCE_PATH = "/api/v1/remote/presence";

export class PushRegistration {
	private current: PushStatus = { kind: "unset" };
	private readonly listeners = new Set<(status: PushStatus) => void>();
	private preference: PushPreference | null = null;
	private running: Promise<PushStatus> | null = null;
	private again = false;
	private stopped = false;

	constructor(
		private readonly channel: PushChannel,
		private readonly platform: PushPlatform,
		private readonly preferences: PreferenceStore,
	) {}

	/** Registers now and on every reconnect. Returns the stop function. */
	start(): () => void {
		this.stopped = false;
		const unsubscribe = this.channel.onState((state) => {
			if (state.kind === "online") void this.sync();
		});
		void this.sync();
		return () => {
			this.stopped = true;
			unsubscribe();
		};
	}

	status(): PushStatus {
		return this.current;
	}

	onStatus(listener: (status: PushStatus) => void): () => void {
		this.listeners.add(listener);
		return () => this.listeners.delete(listener);
	}

	/** Whether to show the explanation screen (never asked, and possible). */
	async shouldOffer(): Promise<boolean> {
		if (this.platform.os === "web") return false;
		if ((await this.loadPreference()) !== "unset") return false;
		const permission = await this.platform.getPermission();
		return permission.status === "granted" || permission.canAskAgain;
	}

	/** The person said yes: ask the OS if needed, then register. */
	async enable(): Promise<PushStatus> {
		if (this.platform.os === "web") return this.set({ kind: "unsupported" });
		let permission = await this.platform.getPermission();
		if (permission.status !== "granted") {
			if (!permission.canAskAgain) {
				await this.platform.openSettings();
				return this.set({ kind: "denied", canAskAgain: false });
			}
			permission = await this.platform.requestPermission();
		}
		if (permission.status !== "granted") {
			// They said no to the system prompt: remember that, do not nag.
			await this.savePreference("off");
			return this.set({ kind: "denied", canAskAgain: permission.canAskAgain });
		}
		await this.savePreference("on");
		return this.sync();
	}

	/** "Agora não" on the explanation screen, or the switch turned off. */
	async disable(): Promise<PushStatus> {
		await this.savePreference("off");
		this.set({ kind: "off" });
		await this.forgetOnDesktop();
		return this.current;
	}

	/**
	 * Brings the desktop in line with the preference: registers (renewing the
	 * lease) when on and allowed, removes the token when not. Calls arriving
	 * while one runs are folded into one more run.
	 */
	sync(): Promise<PushStatus> {
		if (this.running) {
			this.again = true;
			return this.running;
		}
		this.running = (async () => {
			let status: PushStatus;
			do {
				this.again = false;
				status = await this.syncOnce();
			} while (this.again && !this.stopped);
			return status;
		})().finally(() => {
			this.running = null;
		});
		return this.running;
	}

	private async syncOnce(): Promise<PushStatus> {
		if (this.platform.os === "web") return this.set({ kind: "unsupported" });
		try {
			const preference = await this.loadPreference();
			if (preference === "unset") return this.set({ kind: "unset" });
			if (preference === "off") return this.set({ kind: "off" });
			const permission = await this.platform.getPermission();
			if (permission.status !== "granted") {
				// Turned off in the system settings: stop the desktop sending.
				await this.forgetOnDesktop();
				return this.set({ kind: "denied", canAskAgain: permission.canAskAgain });
			}
			await this.platform.prepare();
			const token = await this.platform.getToken();
			if (!token) return this.set({ kind: "unavailable" });
			if (this.channel.state().kind !== "online") return this.set({ kind: "pending" });
			const answer = await this.channel.request<{ expiresAt?: string }>("PUT", PUSH_TOKEN_PATH, {
				token,
				platform: this.platform.os,
			});
			return this.set({ kind: "on", expiresAt: answer?.expiresAt ?? null });
		} catch (error) {
			return this.set({ kind: "error", message: error instanceof Error ? error.message : String(error) });
		}
	}

	/** Removes the token from the desktop without changing the preference. */
	async forgetOnDesktop(): Promise<void> {
		if (this.channel.state().kind !== "online") return;
		await this.channel.request("DELETE", PUSH_TOKEN_PATH).catch(() => {});
	}

	private async loadPreference(): Promise<PushPreference> {
		if (this.preference === null) this.preference = await this.preferences.load().catch(() => "unset" as const);
		return this.preference;
	}

	private async savePreference(preference: PushPreference) {
		this.preference = preference;
		await this.preferences.save(preference).catch(() => {});
	}

	private set(status: PushStatus): PushStatus {
		this.current = status;
		for (const listener of [...this.listeners]) listener(status);
		return status;
	}
}

/** React Native's `AppState` values. */
export type AppVisibility = "active" | "background" | "inactive" | "unknown" | "extension";

export type VisibilitySource = {
	current(): AppVisibility;
	onChange(listener: (state: AppVisibility) => void): () => void;
};

export const PRESENCE_REPEAT_MS = 30_000;

/**
 * Tells the desktop whether the app is on screen: on every connection, on
 * every change, and again every 30 s while visible.
 */
export function reportPresence(
	channel: PushChannel,
	visibility: VisibilitySource,
	repeatMs = PRESENCE_REPEAT_MS,
): () => void {
	let timer: ReturnType<typeof setInterval> | null = null;
	const send = () => {
		if (channel.state().kind !== "online") return;
		const state = visibility.current() === "active" ? "active" : "background";
		void channel.request("PUT", PRESENCE_PATH, { state }).catch(() => {});
	};
	const schedule = () => {
		if (timer) clearInterval(timer);
		timer = visibility.current() === "active" ? setInterval(send, repeatMs) : null;
	};
	const stopState = channel.onState((state) => {
		if (state.kind === "online") send();
	});
	const stopVisibility = visibility.onChange(() => {
		send();
		schedule();
	});
	send();
	schedule();
	return () => {
		stopState();
		stopVisibility();
		if (timer) clearInterval(timer);
	};
}

/**
 * The thread a tapped notification opens. The desktop puts the session id in
 * the payload's `data` (Happy, `notificationRouting.ts`).
 */
export function sessionFromNotificationData(data: unknown): string | null {
	const value =
		typeof data === "string"
			? (() => {
					try {
						return JSON.parse(data) as unknown;
					} catch {
						return null;
					}
				})()
			: data;
	if (!value || typeof value !== "object" || Array.isArray(value)) return null;
	const sessionId = (value as Record<string, unknown>).sessionId;
	if (typeof sessionId !== "string") return null;
	const trimmed = sessionId.trim();
	return trimmed && trimmed.length <= 200 ? trimmed : null;
}
