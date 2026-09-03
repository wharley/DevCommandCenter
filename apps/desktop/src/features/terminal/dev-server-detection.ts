const ANSI_SEQUENCE = /\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))/g;
const LOCAL_URL_CANDIDATE =
	/https?:\/\/(?:localhost|127(?:\.\d{1,3}){3}|0\.0\.0\.0|\[::1?\])(?::\d{1,5})?(?:[/?#][^\s<>"'`\x1b]*)?(?:[),.;\]}])?(?=$|\s|[<>"'`\x1b])/giu;
const TRAILING_TERMINAL_PUNCTUATION = /[),.;\]}]+$/u;

export const DEV_SERVER_DETECTION_TAIL_CHARS = 2_048;

export type DevServerOutputScan = {
	urls: string[];
	tail: string;
};

function isIpv4Loopback(hostname: string): boolean {
	const parts = hostname.split(".");
	if (parts.length !== 4 || parts[0] !== "127") return false;
	return parts.every((part) => {
		if (!/^\d{1,3}$/u.test(part)) return false;
		const value = Number(part);
		return value >= 0 && value <= 255;
	});
}

function normalizeLocalUrl(candidate: string): string | null {
	const trimmed = candidate.replace(TRAILING_TERMINAL_PUNCTUATION, "");
	try {
		const parsed = new URL(trimmed);
		const hostname = parsed.hostname.toLowerCase();
		if (parsed.username || parsed.password) return null;
		if (
			hostname !== "localhost" &&
			hostname !== "[::1]" &&
			hostname !== "[::]" &&
			hostname !== "0.0.0.0" &&
			!isIpv4Loopback(hostname)
		) {
			return null;
		}

		// Bind-all addresses are useful server output but are not destinations.
		if (hostname === "0.0.0.0") parsed.hostname = "127.0.0.1";
		if (hostname === "[::]") parsed.hostname = "[::1]";
		return parsed.href;
	} catch {
		return null;
	}
}

function localUrlMatches(value: string) {
	return [...value.matchAll(LOCAL_URL_CANDIDATE)];
}

/** Detects browser-safe local HTTP(S) URLs without accepting remote terminal output. */
export function detectLocalDevServerUrls(output: string): string[] {
	const plain = output.replace(ANSI_SEQUENCE, "");
	const urls: string[] = [];
	for (const match of localUrlMatches(plain)) {
		const normalized = normalizeLocalUrl(match[0]);
		if (normalized && !urls.includes(normalized)) urls.push(normalized);
	}
	return urls;
}

/** Keeps enough output to recognize a URL split across consecutive PTY chunks. */
export function scanDevServerOutput(
	previousTail: string,
	output: string,
): DevServerOutputScan {
	const plainOutput = output.replace(ANSI_SEQUENCE, "");
	const combined = previousTail + plainOutput;
	const urls: string[] = [];

	for (const match of localUrlMatches(combined)) {
		const matchEnd = (match.index ?? 0) + match[0].length;
		// Ignore URLs wholly contained in the old tail; they were already seen.
		if (matchEnd <= previousTail.length) continue;
		const normalized = normalizeLocalUrl(match[0]);
		if (normalized && !urls.includes(normalized)) urls.push(normalized);
	}

	return {
		urls,
		tail: combined.slice(-DEV_SERVER_DETECTION_TAIL_CHARS),
	};
}

export function formatDevServerAddress(url: string): string {
	try {
		return new URL(url).host;
	} catch {
		return url;
	}
}
