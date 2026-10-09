/**
 * The pairing QR, `dcc://pair#v=1&relay=&room=&pk=&otp=`, and what derives
 * from it; mirrors `dcc_remote::offer`. The secret stays in the fragment and
 * never crosses the relay: it only feeds the Noise PSK.
 */
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { fromBase64Url, toBase64Url, toHex, utf8 } from "./bytes.ts";

export const OFFER_PREFIX = "dcc://pair#";
const HKDF_SALT = utf8("dcc-remote/v1");

export type PairingOffer = {
	relay: string;
	room: string;
	desktopPublicKey: Uint8Array;
	otp: Uint8Array;
};

export function isValidRoomId(room: string): boolean {
	return /^[A-Za-z0-9_-]{22}$/.test(room);
}

export function parsePairingOffer(url: string): PairingOffer {
	if (!url.startsWith(OFFER_PREFIX)) throw new Error("not a dcc://pair link");
	const params = new URLSearchParams(url.slice(OFFER_PREFIX.length));
	if (params.get("v") !== "1") throw new Error("unsupported pairing link version");
	const relay = params.get("relay") ?? "";
	if (!/^wss?:\/\//.test(relay)) throw new Error("relay must be a WebSocket URL");
	const room = params.get("room") ?? "";
	if (!isValidRoomId(room)) throw new Error("invalid room");
	const desktopPublicKey = fromBase64Url(params.get("pk") ?? "");
	if (desktopPublicKey.length !== 32) throw new Error("invalid pk");
	const otp = fromBase64Url(params.get("otp") ?? "");
	if (otp.length !== 16) throw new Error("invalid otp");
	return { relay, room, desktopPublicKey, otp };
}

export function formatPairingOffer(offer: PairingOffer): string {
	const params = new URLSearchParams({
		v: "1",
		relay: offer.relay,
		room: offer.room,
		pk: toBase64Url(offer.desktopPublicKey),
		otp: toBase64Url(offer.otp),
	});
	return OFFER_PREFIX + params.toString();
}

/** The Noise PSK for the pairing handshake. */
export function pskFromOtp(otp: Uint8Array): Uint8Array {
	return hkdf(sha256, otp, HKDF_SALT, utf8("noise psk"), 32);
}

/** Names the offer in handshake message 1 without revealing the secret. */
export function pairIdFromOtp(otp: Uint8Array): string {
	return toHex(hkdf(sha256, otp, HKDF_SALT, utf8("pair id"), 8));
}

/** Short form of a static key both screens show, e.g. `3F9A-0C12-B7E4`. */
export function fingerprint(publicKey: Uint8Array): string {
	const hex = toHex(sha256(publicKey).subarray(0, 6)).toUpperCase();
	return `${hex.slice(0, 4)}-${hex.slice(4, 8)}-${hex.slice(8, 12)}`;
}
