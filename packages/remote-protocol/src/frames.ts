/** Application frames; mirrors `dcc_remote::frames`. */
import { fromBase64, fromUtf8, toBase64, utf8 } from "./bytes.ts";

export type Headers = Record<string, string>;

type BodyFields = { body?: string; bodyEncoding?: "base64" };

export type RequestFrame = {
	t: "req";
	id: number;
	method: string;
	path: string;
	headers?: Headers;
} & BodyFields;

export type ResponseFrame = {
	t: "res";
	id: number;
	status: number;
	headers?: Headers;
} & BodyFields;

export type SubscribeFrame = { t: "sub"; id: number; path: string };
export type EventFrame = { t: "evt"; id: number; event: string; data: unknown; seq?: number };
export type EndFrame = { t: "end"; id: number; error?: string };
export type CancelFrame = { t: "cancel"; id: number };
export type PingFrame = { t: "ping"; ts?: number };
export type PongFrame = { t: "pong"; ts?: number };
export type PairingStatus = "pending" | "approved" | "denied";
export type PairingFrame = { t: "pairing"; status: PairingStatus; deviceId?: string };

export type Frame =
	| RequestFrame
	| ResponseFrame
	| SubscribeFrame
	| EventFrame
	| EndFrame
	| CancelFrame
	| PingFrame
	| PongFrame
	| PairingFrame;

const FRAME_TYPES = new Set(["req", "res", "sub", "evt", "end", "cancel", "ping", "pong", "pairing"]);

export function encodeFrame(frame: Frame): Uint8Array {
	return utf8(JSON.stringify(frame));
}

export function decodeFrame(bytes: Uint8Array): Frame {
	const value: unknown = JSON.parse(fromUtf8(bytes));
	if (
		typeof value !== "object" ||
		value === null ||
		!FRAME_TYPES.has((value as { t?: unknown }).t as string)
	) {
		throw new Error("unknown frame");
	}
	return value as Frame;
}

export function bodyFields(body: Uint8Array | string | undefined): BodyFields {
	if (body === undefined) return {};
	if (typeof body === "string") return body.length ? { body } : {};
	if (body.length === 0) return {};
	try {
		return { body: fromUtf8(body) };
	} catch {
		return { body: toBase64(body), bodyEncoding: "base64" };
	}
}

export function bodyBytes(fields: BodyFields): Uint8Array {
	if (fields.body === undefined) return new Uint8Array(0);
	return fields.bodyEncoding === "base64" ? fromBase64(fields.body) : utf8(fields.body);
}
