/**
 * Noise Protocol Framework (revision 34), only what DCC uses:
 * `Noise_IK_25519_ChaChaPoly_SHA256` and `Noise_IKpsk2_25519_ChaChaPoly_SHA256`.
 *
 * Checked against the public cacophony vectors and against vectors that
 * snow (the Rust side) generates; see `test/noise.test.ts`.
 */
import { chacha20poly1305 } from "@noble/ciphers/chacha.js";
import { x25519 } from "@noble/curves/ed25519.js";
import { hmac } from "@noble/hashes/hmac.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { concat, equalBytes, utf8 } from "./bytes.ts";

export const PAIR_PATTERN = "Noise_IKpsk2_25519_ChaChaPoly_SHA256";
export const RESUME_PATTERN = "Noise_IK_25519_ChaChaPoly_SHA256";
/** Bound into every handshake; mirrors `dcc_remote::noise::PROLOGUE`. */
export const PROLOGUE = utf8("dcc-remote/v1");

const DH_LEN = 32;
const HASH_LEN = 32;
export const TAG_LEN = 16;
export const MAX_NOISE_MESSAGE = 65535;
const EMPTY = new Uint8Array(0);

type Token = "e" | "s" | "ee" | "es" | "se" | "ss" | "psk";

/** Both patterns pre-share the responder's static key (`<- s`). */
const MESSAGES: Record<string, Token[][]> = {
	[RESUME_PATTERN]: [
		["e", "es", "s", "ss"],
		["e", "ee", "se"],
	],
	[PAIR_PATTERN]: [
		["e", "es", "s", "ss"],
		["e", "ee", "se", "psk"],
	],
};

export type KeyPair = { privateKey: Uint8Array; publicKey: Uint8Array };

export function generateKeyPair(): KeyPair {
	return keyPairFromPrivate(x25519.utils.randomSecretKey());
}

export function keyPairFromPrivate(privateKey: Uint8Array): KeyPair {
	if (privateKey.length !== DH_LEN) throw new Error("private key must be 32 bytes");
	return { privateKey, publicKey: x25519.getPublicKey(privateKey) };
}

function dh(keyPair: KeyPair, publicKey: Uint8Array): Uint8Array {
	return x25519.getSharedSecret(keyPair.privateKey, publicKey);
}

/** Noise HKDF: HMAC-SHA256 chained, returning `count` outputs. */
function hkdf(chainingKey: Uint8Array, ikm: Uint8Array, count: 2 | 3): Uint8Array[] {
	const temp = hmac(sha256, chainingKey, ikm);
	const out1 = hmac(sha256, temp, Uint8Array.of(1));
	const out2 = hmac(sha256, temp, concat(out1, Uint8Array.of(2)));
	if (count === 2) return [out1, out2];
	return [out1, out2, hmac(sha256, temp, concat(out2, Uint8Array.of(3)))];
}

function nonceBytes(n: number): Uint8Array {
	const nonce = new Uint8Array(12);
	const view = new DataView(nonce.buffer);
	view.setUint32(4, n >>> 0, true);
	view.setUint32(8, Math.floor(n / 0x1_0000_0000), true);
	return nonce;
}

export class CipherState {
	private key: Uint8Array | null;
	private nonce = 0;

	constructor(key: Uint8Array | null = null) {
		this.key = key;
	}

	hasKey(): boolean {
		return this.key !== null;
	}

	encryptWithAd(ad: Uint8Array, plaintext: Uint8Array): Uint8Array {
		if (!this.key) return plaintext;
		if (this.nonce >= Number.MAX_SAFE_INTEGER) throw new Error("nonce exhausted");
		const out = chacha20poly1305(this.key, nonceBytes(this.nonce), ad).encrypt(plaintext);
		this.nonce += 1;
		return out;
	}

	decryptWithAd(ad: Uint8Array, ciphertext: Uint8Array): Uint8Array {
		if (!this.key) return ciphertext;
		if (this.nonce >= Number.MAX_SAFE_INTEGER) throw new Error("nonce exhausted");
		let out: Uint8Array;
		try {
			out = chacha20poly1305(this.key, nonceBytes(this.nonce), ad).decrypt(ciphertext);
		} catch {
			throw new Error("noise: decrypt failed");
		}
		this.nonce += 1;
		return out;
	}
}

class SymmetricState {
	chainingKey: Uint8Array;
	hash: Uint8Array;
	cipher = new CipherState();

	constructor(protocolName: string) {
		const name = utf8(protocolName);
		if (name.length <= HASH_LEN) {
			this.hash = new Uint8Array(HASH_LEN);
			this.hash.set(name);
		} else {
			this.hash = sha256(name);
		}
		this.chainingKey = this.hash;
	}

	mixKey(ikm: Uint8Array) {
		const [chainingKey, key] = hkdf(this.chainingKey, ikm, 2);
		this.chainingKey = chainingKey!;
		this.cipher = new CipherState(key!);
	}

	mixHash(data: Uint8Array) {
		this.hash = sha256(concat(this.hash, data));
	}

	mixKeyAndHash(ikm: Uint8Array) {
		const [chainingKey, tempHash, key] = hkdf(this.chainingKey, ikm, 3);
		this.chainingKey = chainingKey!;
		this.mixHash(tempHash!);
		this.cipher = new CipherState(key!);
	}

	encryptAndHash(plaintext: Uint8Array): Uint8Array {
		const ciphertext = this.cipher.encryptWithAd(this.hash, plaintext);
		this.mixHash(ciphertext);
		return ciphertext;
	}

	decryptAndHash(ciphertext: Uint8Array): Uint8Array {
		const plaintext = this.cipher.decryptWithAd(this.hash, ciphertext);
		this.mixHash(ciphertext);
		return plaintext;
	}

	split(): [CipherState, CipherState] {
		const [k1, k2] = hkdf(this.chainingKey, EMPTY, 2);
		return [new CipherState(k1!), new CipherState(k2!)];
	}
}

export type HandshakeOptions = {
	protocolName: string;
	initiator: boolean;
	prologue: Uint8Array;
	localStatic: KeyPair;
	/** The responder's static key; the initiator must know it (IK). */
	remoteStatic?: Uint8Array;
	psk?: Uint8Array;
	/** Fixed ephemeral key, for test vectors only. */
	localEphemeral?: KeyPair;
};

export type TransportCiphers = { send: CipherState; receive: CipherState };

export class HandshakeState {
	private readonly symmetric: SymmetricState;
	private readonly messages: Token[][];
	private readonly initiator: boolean;
	private readonly isPsk: boolean;
	private readonly s: KeyPair;
	private e: KeyPair | null = null;
	private rs: Uint8Array | null;
	private re: Uint8Array | null = null;
	private psk: Uint8Array | null;
	private readonly fixedEphemeral: KeyPair | null;
	private index = 0;

	constructor(options: HandshakeOptions) {
		const messages = MESSAGES[options.protocolName];
		if (!messages) throw new Error(`unsupported Noise protocol ${options.protocolName}`);
		this.messages = messages;
		this.initiator = options.initiator;
		this.isPsk = options.protocolName.includes("psk");
		this.s = options.localStatic;
		this.rs = options.remoteStatic ?? null;
		this.psk = options.psk ?? null;
		this.fixedEphemeral = options.localEphemeral ?? null;
		if (this.initiator && !this.rs) throw new Error("IK initiator needs the responder key");
		this.symmetric = new SymmetricState(options.protocolName);
		this.symmetric.mixHash(options.prologue);
		// Pre-message `<- s`.
		this.symmetric.mixHash(this.initiator ? this.rs! : this.s.publicKey);
	}

	setPsk(psk: Uint8Array) {
		if (psk.length !== 32) throw new Error("psk must be 32 bytes");
		this.psk = psk;
	}

	get remoteStatic(): Uint8Array | null {
		return this.rs;
	}

	get handshakeHash(): Uint8Array {
		return this.symmetric.hash;
	}

	get finished(): boolean {
		return this.index >= this.messages.length;
	}

	private myTurn(): boolean {
		return (this.index % 2 === 0) === this.initiator;
	}

	private mixDh(token: "ee" | "es" | "se" | "ss") {
		const e = this.e;
		const re = this.re;
		const rs = this.rs;
		const need = <T>(value: T | null): T => {
			if (value === null) throw new Error(`noise: missing key for ${token}`);
			return value;
		};
		switch (token) {
			case "ee":
				this.symmetric.mixKey(dh(need(e), need(re)));
				break;
			case "es":
				this.symmetric.mixKey(this.initiator ? dh(need(e), need(rs)) : dh(this.s, need(re)));
				break;
			case "se":
				this.symmetric.mixKey(this.initiator ? dh(this.s, need(re)) : dh(need(e), need(rs)));
				break;
			case "ss":
				this.symmetric.mixKey(dh(this.s, need(rs)));
				break;
		}
	}

	private mixPsk() {
		if (!this.psk) throw new Error("noise: psk not set");
		this.symmetric.mixKeyAndHash(this.psk);
	}

	writeMessage(payload: Uint8Array): Uint8Array {
		if (this.finished || !this.myTurn()) throw new Error("noise: not our turn to write");
		const parts: Uint8Array[] = [];
		for (const token of this.messages[this.index]!) {
			switch (token) {
				case "e": {
					this.e = this.fixedEphemeral ?? generateKeyPair();
					parts.push(this.e.publicKey);
					this.symmetric.mixHash(this.e.publicKey);
					if (this.isPsk) this.symmetric.mixKey(this.e.publicKey);
					break;
				}
				case "s":
					parts.push(this.symmetric.encryptAndHash(this.s.publicKey));
					break;
				case "psk":
					this.mixPsk();
					break;
				default:
					this.mixDh(token);
			}
		}
		parts.push(this.symmetric.encryptAndHash(payload));
		this.index += 1;
		const message = concat(...parts);
		if (message.length > MAX_NOISE_MESSAGE) throw new Error("noise: message too large");
		return message;
	}

	readMessage(message: Uint8Array): Uint8Array {
		if (this.finished || this.myTurn()) throw new Error("noise: not our turn to read");
		if (message.length > MAX_NOISE_MESSAGE) throw new Error("noise: message too large");
		let offset = 0;
		const take = (length: number) => {
			if (offset + length > message.length) throw new Error("noise: message too short");
			const out = message.subarray(offset, offset + length);
			offset += length;
			return out;
		};
		for (const token of this.messages[this.index]!) {
			switch (token) {
				case "e": {
					this.re = take(DH_LEN).slice();
					this.symmetric.mixHash(this.re);
					if (this.isPsk) this.symmetric.mixKey(this.re);
					break;
				}
				case "s": {
					const length = this.symmetric.cipher.hasKey() ? DH_LEN + TAG_LEN : DH_LEN;
					const rs = this.symmetric.decryptAndHash(take(length));
					if (this.rs && !equalBytes(this.rs, rs)) throw new Error("noise: unexpected static key");
					this.rs = rs;
					break;
				}
				case "psk":
					this.mixPsk();
					break;
				default:
					this.mixDh(token);
			}
		}
		const payload = this.symmetric.decryptAndHash(message.subarray(offset));
		this.index += 1;
		return payload;
	}

	/** After the last message: the transport ciphers for this side. */
	split(): TransportCiphers {
		if (!this.finished) throw new Error("noise: handshake not finished");
		const [first, second] = this.symmetric.split();
		return this.initiator ? { send: first, receive: second } : { send: second, receive: first };
	}
}
