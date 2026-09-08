import type { PairingSession } from "./session";

const PREFIX = "dcc-mobile:";
function key(session: PairingSession, name: string): string {
	return `${PREFIX}${encodeURIComponent(session.backendUrl)}:${session.deviceId}:${name}`;
}
export function readLocal<T>(session: PairingSession, name: string): T | null {
	try {
		return JSON.parse(
			localStorage.getItem(key(session, name)) ?? "null",
		) as T | null;
	} catch {
		return null;
	}
}
export function writeLocal(
	session: PairingSession,
	name: string,
	value: unknown,
): void {
	try {
		const encoded = JSON.stringify(value);
		if (encoded.length <= 500_000)
			localStorage.setItem(key(session, name), encoded);
	} catch {
		/* Storage may be full or disabled; network work stays available. */
	}
}
export function removeLocal(session: PairingSession, name: string): void {
	try {
		localStorage.removeItem(key(session, name));
	} catch {
		/* best effort */
	}
}
export function clearLocalData(): void {
	try {
		for (const name of Object.keys(localStorage))
			if (name.startsWith(PREFIX)) localStorage.removeItem(name);
	} catch {
		/* best effort */
	}
}
