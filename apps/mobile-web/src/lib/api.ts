import type { PairingSession } from "./session";

/**
 * Thin fetch wrapper that injects `Authorization: Bearer <token>` from the
 * persisted pairing session, throws on non-2xx responses, and returns parsed
 * JSON. Designed to be called from React components inside a `useEffect`.
 */
export class ApiError extends Error {
	readonly status: number;
	constructor(status: number, message: string) {
		super(message);
		this.status = status;
		this.name = "ApiError";
	}
}

async function parseError(res: Response): Promise<string> {
	try {
		const body = await res.json();
		if (body && typeof body === "object" && body.error && typeof body.error.message === "string") {
			return body.error.message;
		}
	} catch {
		/* ignore */
	}
	return `HTTP ${res.status}`;
}

export async function apiFetch<T>(
	session: PairingSession,
	path: string,
	init: RequestInit = {},
): Promise<T> {
	const headers = new Headers(init.headers);
	headers.set("Authorization", `Bearer ${session.sessionToken}`);
	if (init.body && !headers.has("Content-Type")) {
		headers.set("Content-Type", "application/json");
	}

	const url = `${session.backendUrl}${path.startsWith("/") ? path : `/${path}`}`;
	const res = await fetch(url, { ...init, headers });
	if (!res.ok) {
		throw new ApiError(res.status, await parseError(res));
	}
	if (res.status === 204) {
		return undefined as T;
	}
	const contentType = res.headers.get("content-type") ?? "";
	if (!contentType.includes("json")) {
		return (await res.text()) as unknown as T;
	}
	return (await res.json()) as T;
}
