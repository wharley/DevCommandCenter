/**
 * Wire envelope and transport encryption; mirrors `dcc_remote::{wire, channel}`.
 *
 * ```
 * [0x01][mode][noise handshake message 1]   phone -> desktop
 * [0x02][noise handshake message 2]         desktop -> phone
 * [0x03][noise transport message]           both ways
 * ```
 *
 * A Noise message holds at most 65535 bytes, so an application message is cut
 * into chunks: each chunk's plaintext is a flag byte (bit 0 = last chunk) and
 * data. Implicit nonces make a dropped or reordered chunk fail to decrypt.
 */
import { concat } from "./bytes.ts";
import { type CipherState, MAX_NOISE_MESSAGE, TAG_LEN } from "./noise.ts";

export const KIND_HANDSHAKE_INIT = 0x01;
export const KIND_HANDSHAKE_RESPONSE = 0x02;
export const KIND_TRANSPORT = 0x03;

export const MODE_PAIR = 1;
export const MODE_RESUME = 2;
export type Mode = typeof MODE_PAIR | typeof MODE_RESUME;

export const MAX_CHUNK = MAX_NOISE_MESSAGE - TAG_LEN - 1;
export const MAX_APP_MESSAGE = 32 * 1024 * 1024;
const FLAG_FINAL = 0x01;
const EMPTY_AD = new Uint8Array(0);

/** Close codes the desktop uses to refuse a phone (`dcc_remote::wire::close`). */
export const CloseCode = {
	PROTOCOL: 4400,
	UNKNOWN_DEVICE: 4401,
	PAIRING_DENIED: 4403,
	OFFER_INVALID: 4410,
	DEVICE_LIMIT: 4429,
	REVOKED: 4499,
} as const;

export function encodeHandshakeInit(mode: Mode, noise: Uint8Array): Uint8Array {
	return concat(Uint8Array.of(KIND_HANDSHAKE_INIT, mode), noise);
}

export function decodeHandshakeResponse(wire: Uint8Array): Uint8Array {
	if (wire[0] !== KIND_HANDSHAKE_RESPONSE) throw new Error("expected a handshake response");
	return wire.subarray(1);
}

export class SecureChannel {
	private readonly send: CipherState;
	private readonly receive: CipherState;
	private partial: Uint8Array[] = [];
	private partialLength = 0;

	constructor(ciphers: { send: CipherState; receive: CipherState }) {
		this.send = ciphers.send;
		this.receive = ciphers.receive;
	}

	/** Encrypts one application message into the wire messages to send, in order. */
	seal(message: Uint8Array): Uint8Array[] {
		if (message.length > MAX_APP_MESSAGE) throw new Error("message too large");
		const out: Uint8Array[] = [];
		if (message.length === 0) out.push(this.sealChunk(message, true));
		for (let offset = 0; offset < message.length; offset += MAX_CHUNK) {
			const chunk = message.subarray(offset, offset + MAX_CHUNK);
			out.push(this.sealChunk(chunk, offset + MAX_CHUNK >= message.length));
		}
		return out;
	}

	private sealChunk(data: Uint8Array, last: boolean): Uint8Array {
		const plaintext = concat(Uint8Array.of(last ? FLAG_FINAL : 0), data);
		return concat(Uint8Array.of(KIND_TRANSPORT), this.send.encryptWithAd(EMPTY_AD, plaintext));
	}

	/** Opens one transport wire message; returns the application message once complete. */
	open(wire: Uint8Array): Uint8Array | null {
		if (wire[0] !== KIND_TRANSPORT) throw new Error("not a transport message");
		const noise = wire.subarray(1);
		if (noise.length > MAX_NOISE_MESSAGE || noise.length < TAG_LEN + 1) {
			throw new Error("transport message size");
		}
		const plaintext = this.receive.decryptWithAd(EMPTY_AD, noise);
		const flags = plaintext[0]!;
		const data = plaintext.subarray(1);
		if (this.partialLength + data.length > MAX_APP_MESSAGE) {
			this.partial = [];
			this.partialLength = 0;
			throw new Error("message too large");
		}
		this.partial.push(data);
		this.partialLength += data.length;
		if ((flags & FLAG_FINAL) === 0) return null;
		const message = concat(...this.partial);
		this.partial = [];
		this.partialLength = 0;
		return message;
	}
}
