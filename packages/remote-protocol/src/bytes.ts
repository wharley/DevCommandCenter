const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

export function utf8(text: string): Uint8Array {
	return encoder.encode(text);
}

export function fromUtf8(bytes: Uint8Array): string {
	return decoder.decode(bytes);
}

export function concat(...parts: Uint8Array[]): Uint8Array {
	let length = 0;
	for (const part of parts) length += part.length;
	const out = new Uint8Array(length);
	let offset = 0;
	for (const part of parts) {
		out.set(part, offset);
		offset += part.length;
	}
	return out;
}

export function toHex(bytes: Uint8Array): string {
	let out = "";
	for (const byte of bytes) out += byte.toString(16).padStart(2, "0");
	return out;
}

export function fromHex(hex: string): Uint8Array {
	if (hex.length % 2 !== 0 || /[^0-9a-f]/i.test(hex)) {
		throw new Error("invalid hex");
	}
	const out = new Uint8Array(hex.length / 2);
	for (let i = 0; i < out.length; i++) {
		out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
	}
	return out;
}

const B64URL = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/** Unpadded base64url, as the QR carries keys and secrets. */
export function toBase64Url(bytes: Uint8Array): string {
	let out = "";
	let i = 0;
	for (; i + 2 < bytes.length; i += 3) {
		const n = (bytes[i]! << 16) | (bytes[i + 1]! << 8) | bytes[i + 2]!;
		out += B64URL[n >> 18]! + B64URL[(n >> 12) & 63]! + B64URL[(n >> 6) & 63]! + B64URL[n & 63]!;
	}
	const rest = bytes.length - i;
	if (rest === 1) {
		const n = bytes[i]! << 16;
		out += B64URL[n >> 18]! + B64URL[(n >> 12) & 63]!;
	} else if (rest === 2) {
		const n = (bytes[i]! << 16) | (bytes[i + 1]! << 8);
		out += B64URL[n >> 18]! + B64URL[(n >> 12) & 63]! + B64URL[(n >> 6) & 63]!;
	}
	return out;
}

export function fromBase64Url(text: string): Uint8Array {
	if (text.length % 4 === 1 || /[^A-Za-z0-9_-]/.test(text)) {
		throw new Error("invalid base64url");
	}
	const out = new Uint8Array(Math.floor((text.length * 3) / 4));
	let bits = 0;
	let value = 0;
	let index = 0;
	for (const char of text) {
		value = (value << 6) | B64URL.indexOf(char);
		bits += 6;
		if (bits >= 8) {
			bits -= 8;
			out[index++] = (value >> bits) & 0xff;
		}
	}
	return out;
}

/** Standard base64 with padding, for binary HTTP bodies. */
export function toBase64(bytes: Uint8Array): string {
	const url = toBase64Url(bytes).replace(/-/g, "+").replace(/_/g, "/");
	return url + "=".repeat((4 - (url.length % 4)) % 4);
}

export function fromBase64(text: string): Uint8Array {
	return fromBase64Url(text.replace(/=+$/, "").replace(/\+/g, "-").replace(/\//g, "_"));
}

export function equalBytes(a: Uint8Array, b: Uint8Array): boolean {
	if (a.length !== b.length) return false;
	let diff = 0;
	for (let i = 0; i < a.length; i++) diff |= a[i]! ^ b[i]!;
	return diff === 0;
}
