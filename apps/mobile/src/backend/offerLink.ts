/**
 * A `dcc://pair#…` link that opened the app. The secret lives in the URL
 * fragment, which the router drops, so the raw URL is kept here (from
 * `app/+native-intent.tsx` and the Linking listener) until the pairing screen
 * takes it — once. With the app already open both of those see the same link,
 * and the otp is single-use: a second attempt would fail with 4410.
 */
import { OFFER_PREFIX } from "@dcc/remote-protocol";

let pending: string | null = null;
const seen = new Set<string>();
const listeners = new Set<(url: string) => void>();

export function isPairingLink(url: string | null | undefined): url is string {
	return typeof url === "string" && url.startsWith(OFFER_PREFIX);
}

export function stashPairingLink(url: string): void {
	if (!isPairingLink(url) || seen.has(url)) return;
	seen.add(url);
	pending = url;
	for (const listener of [...listeners]) listener(url);
}

export function takePairingLink(): string | null {
	const url = pending;
	pending = null;
	return url;
}

export function onPairingLink(listener: (url: string) => void): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}
