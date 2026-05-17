/**
 * Persisted pairing session. After the user pairs once we keep the bearer
 * token + backend URL in IndexedDB so the app reopens straight into the home.
 */
export type PairingSession = {
	deviceId: string;
	sessionToken: string;
	backendUrl: string;
	createdAt: string;
};

const DB_NAME = "dcc-pair";
const STORE = "device";
const KEY = "current";

function openDb(): Promise<IDBDatabase> {
	return new Promise((resolve, reject) => {
		const req = indexedDB.open(DB_NAME, 1);
		req.onupgradeneeded = () => req.result.createObjectStore(STORE);
		req.onsuccess = () => resolve(req.result);
		req.onerror = () => reject(req.error);
	});
}

export async function loadSession(): Promise<PairingSession | null> {
	try {
		const db = await openDb();
		return await new Promise((resolve) => {
			const tx = db.transaction(STORE, "readonly");
			const req = tx.objectStore(STORE).get(KEY);
			req.onsuccess = () => resolve((req.result as PairingSession | undefined) ?? null);
			req.onerror = () => resolve(null);
		});
	} catch {
		return null;
	}
}

export async function saveSession(session: PairingSession): Promise<void> {
	const db = await openDb();
	await new Promise<void>((resolve, reject) => {
		const tx = db.transaction(STORE, "readwrite");
		tx.objectStore(STORE).put(session, KEY);
		tx.oncomplete = () => resolve();
		tx.onerror = () => reject(tx.error);
	});
}

export async function clearSession(): Promise<void> {
	try {
		const db = await openDb();
		await new Promise<void>((resolve) => {
			const tx = db.transaction(STORE, "readwrite");
			tx.objectStore(STORE).delete(KEY);
			tx.oncomplete = () => resolve();
			tx.onerror = () => resolve();
		});
	} catch {
		/* ignore */
	}
}

/**
 * Backend URL the SPA should call. Two sources, in priority:
 *   1. URL hash override `#be=http://...` — set when the desktop QR is scanned
 *      from a different LAN address than the SPA was originally loaded from
 *   2. The window origin — works when the SPA is served by the backend itself
 *      (production case under /m/)
 */
export function resolveBackendOrigin(): string {
	const hash = window.location.hash.startsWith("#")
		? window.location.hash.slice(1)
		: "";
	const params = new URLSearchParams(hash);
	const override = params.get("be");
	if (override && /^https?:\/\//.test(override)) {
		return override.replace(/\/$/, "");
	}
	return window.location.origin;
}
