/**
 * What the relay enforces. It forwards bytes between the desktop and phones
 * and knows nothing of their content: the frames are Noise ciphertext
 * (see `crates/dcc-remote` and `packages/remote-protocol`).
 *
 * Routes, all WebSocket upgrades:
 * - `/v1/rooms/<room>/control`: the desktop's control connection. Needs the
 *   room secret in `x-dcc-room-secret`; the first one registers the room.
 *   The relay sends it `{"type":"connected"|"disconnected","connectionId"}`.
 * - `/v1/rooms/<room>/connect`: a phone. No relay credential: the desktop
 *   authenticates it through Noise.
 * - `/v1/rooms/<room>/data/<connectionId>`: the desktop's data connection for
 *   one phone, opened after `connected`. Needs the room secret.
 */

export const ROOM_ID = /^[A-Za-z0-9_-]{22}$/;
export const CONNECTION_ID = /^[0-9a-f]{32}$/;

/** Kind byte + mode byte + one Noise message. */
export const MAX_FRAME_BYTES = 2 + 65535;
export const MAX_PHONES_PER_ROOM = 8;
/** Phone frames held while the desktop opens the data connection. */
export const MAX_BUFFERED_FRAMES = 16;
export const MAX_BUFFERED_BYTES = 256 * 1024;
/** Phones are untrusted; the room owner's own frames are not throttled. */
export const PHONE_FRAMES_PER_SECOND = 100;
export const PHONE_FRAME_BURST = 300;
/** A room nobody controlled for this long is forgotten. */
export const ROOM_TTL_MS = 30 * 24 * 60 * 60 * 1000;

/** Close codes the relay itself uses (the desktop's are 4400–4499). */
export const RelayClose = {
	DESKTOP_OFFLINE: 4603,
	FORBIDDEN: 4601,
	FRAME_TOO_LARGE: 4613,
	TOO_MANY_CONNECTIONS: 4629,
	RATE_LIMITED: 4628,
	REPLACED: 4609,
	BAD_FRAME: 4600,
} as const;

export type Route =
	| { kind: "control"; room: string }
	| { kind: "connect"; room: string }
	| { kind: "data"; room: string; connectionId: string }
	| { kind: "traffic"; room: string };

export function parseRoute(pathname: string): Route | null {
	const parts = pathname.split("/").filter(Boolean);
	if (parts[0] === "debug" && parts[1] === "rooms" && parts[3] === "traffic" && parts.length === 4) {
		return ROOM_ID.test(parts[2]!) ? { kind: "traffic", room: parts[2]! } : null;
	}
	if (parts[0] !== "v1" || parts[1] !== "rooms" || !parts[2] || !ROOM_ID.test(parts[2])) return null;
	const room = parts[2];
	if (parts.length === 4 && parts[3] === "control") return { kind: "control", room };
	if (parts.length === 4 && parts[3] === "connect") return { kind: "connect", room };
	if (parts.length === 5 && parts[3] === "data" && CONNECTION_ID.test(parts[4]!)) {
		return { kind: "data", room, connectionId: parts[4]! };
	}
	return null;
}

/** Token bucket; `take` answers whether one more frame fits now. */
export class TokenBucket {
	private tokens: number;
	private updatedAt: number;
	private readonly rate: number;
	private readonly burst: number;

	constructor(rate: number, burst: number, now: number) {
		this.rate = rate;
		this.burst = burst;
		this.tokens = burst;
		this.updatedAt = now;
	}

	take(now: number): boolean {
		this.tokens = Math.min(this.burst, this.tokens + ((now - this.updatedAt) / 1000) * this.rate);
		this.updatedAt = now;
		if (this.tokens < 1) return false;
		this.tokens -= 1;
		return true;
	}
}

export async function sha256Hex(text: string): Promise<string> {
	const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

export function constantTimeEqual(a: string, b: string): boolean {
	if (a.length !== b.length) return false;
	let diff = 0;
	for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
	return diff === 0;
}

/** A close code from one side that is safe to hand to the other. */
export function forwardableCloseCode(code: number): number {
	return code >= 4000 && code <= 4999 ? code : 1000;
}
