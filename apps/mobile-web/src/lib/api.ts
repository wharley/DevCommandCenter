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
		if (
			body &&
			typeof body === "object" &&
			body.error &&
			typeof body.error.message === "string"
		) {
			return body.error.message;
		}
	} catch {
		/* ignore */
	}
	if (res.status === 401 || res.status === 403)
		return "O pareamento expirou ou foi revogado. Pareie novamente nas configurações do desktop.";
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
	const controller = new AbortController();
	const abort = () => controller.abort();
	init.signal?.addEventListener("abort", abort, { once: true });
	if (init.signal?.aborted) controller.abort();
	const timer = setTimeout(abort, 60_000);
	try {
		const res = await fetch(url, {
			...init,
			headers,
			cache: "no-store",
			signal: controller.signal,
		});
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
	} catch (error) {
		if (error instanceof ApiError) throw error;
		if (init.signal?.aborted) throw error;
		throw new Error(
			controller.signal.aborted
				? "O computador demorou para responder. Confira a conexão; uma ação enviada pode ainda estar em andamento."
				: "Não foi possível alcançar o computador. Confira a rede e o Tailscale.",
		);
	} finally {
		clearTimeout(timer);
		init.signal?.removeEventListener("abort", abort);
	}
}
