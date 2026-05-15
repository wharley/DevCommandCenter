import {
	readActiveRemoteEnvironmentId,
	readRemoteEnvironments,
	type SavedRemoteEnvironment,
} from "@/features/settings/remote-environments-store";

export type RemoteBackendHealth = {
	status: string;
	daemon: string;
	database?: string;
	daemonHealth?: unknown;
	error?: {
		code?: string;
		message?: string;
	};
};

export type RemoteBackendStatus = unknown;

export function getActiveRemoteEnvironment(): SavedRemoteEnvironment | null {
	const activeId = readActiveRemoteEnvironmentId();
	if (!activeId) {
		return null;
	}
	return (
		readRemoteEnvironments().find((environment) => environment.id === activeId) ?? null
	);
}

async function remoteFetch<T>(
	path: string,
	environment: SavedRemoteEnvironment,
	init?: RequestInit,
): Promise<T> {
	const endpoint = environment.endpoint?.trim();
	const bearerToken = environment.bearerToken?.trim();
	if (!endpoint || !bearerToken) {
		throw new Error("Remote environment is missing endpoint or bearer token.");
	}

	const url = new URL(path, endpoint.endsWith("/") ? endpoint : `${endpoint}/`);
	const response = await fetch(url, {
		...init,
		headers: {
			Authorization: `Bearer ${bearerToken}`,
			"Content-Type": "application/json",
			...(init?.headers ?? {}),
		},
	});

	let body: unknown = null;
	try {
		body = await response.json();
	} catch {
		// ignore malformed payloads, we'll fail below on status
	}

	if (!response.ok) {
		const message =
			typeof body === "object" &&
			body &&
			"error" in body &&
			typeof (body as { error?: unknown }).error === "object" &&
			(body as { error?: { message?: string } }).error?.message
				? (body as { error?: { message?: string } }).error?.message
				: `Remote backend returned HTTP ${response.status}`;
		throw new Error(message);
	}

	return body as T;
}

export function fetchRemoteBackendHealth(environment: SavedRemoteEnvironment) {
	return remoteFetch<RemoteBackendHealth>("/health", environment, {
		method: "GET",
	});
}

export function fetchRemoteBackendStatus(environment: SavedRemoteEnvironment) {
	return remoteFetch<RemoteBackendStatus>("/api/v1/status", environment, {
		method: "GET",
	});
}
