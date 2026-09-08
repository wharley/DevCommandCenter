import { apiFetch } from "./api";
import { readLocal, writeLocal, removeLocal } from "./local-data";
import type { PairingSession } from "./session";

export type MobileCatalog = {
	providers: {
		id: string;
		label: string;
		enabled: boolean;
		health:
			| string
			| { Unhealthy?: { reason: string }; Degraded?: { reason: string } };
		models: { id: string; label: string; recommended: boolean }[];
	}[];
	repositories: {
		id: string;
		name: string;
		rootPath: string;
		baseBranch: string;
		projectId: string;
	}[];
	workspaces: {
		id: string;
		name: string | null;
		rootPath: string;
		baseBranch: string;
		projectId: string;
		state: string;
	}[];
};
export type TaskInput = {
	requestId: string;
	workspaceId: string | null;
	repositoryId: string | null;
	title: string;
	prompt: string;
	providerId: string;
	model: string | null;
	planMode: boolean;
};
export type TaskResult = {
	state: "creating" | "started" | "failed";
	sessionId?: string;
	workspaceId?: string;
	error?: string;
};
export const pendingTask = (session: PairingSession) =>
	readLocal<TaskInput>(session, "pending-task");
export const forgetTask = (session: PairingSession) =>
	removeLocal(session, "pending-task");

/** The same durable request ID is retained across reloads and ambiguous network failures. */
export async function createMobileTask(
	session: PairingSession,
	input: TaskInput,
	signal?: AbortSignal,
): Promise<TaskResult> {
	writeLocal(session, "pending-task", input);
	if (pendingTask(session)?.requestId !== input.requestId)
		throw new Error(
			"Libere espaço ou habilite o armazenamento do navegador para salvar esta criação antes do envio.",
		);
	let result = await apiFetch<TaskResult>(session, "/api/v1/mobile/tasks", {
		method: "POST",
		body: JSON.stringify(input),
		signal,
	});
	for (
		let attempts = 0;
		result.state === "creating" && attempts < 90;
		attempts++
	) {
		await new Promise<void>((resolve, reject) => {
			const abort = () => {
				clearTimeout(timer);
				reject(new DOMException("Aborted", "AbortError"));
			};
			const timer = setTimeout(() => {
				signal?.removeEventListener("abort", abort);
				resolve();
			}, 1_000);
			if (signal?.aborted) abort();
			else signal?.addEventListener("abort", abort, { once: true });
		});
		result = await apiFetch<TaskResult>(
			session,
			`/api/v1/mobile/tasks/${input.requestId}`,
			{ signal },
		);
	}
	return result;
}

export function newRequestId(): string {
	// randomUUID is unavailable over plain HTTP; getRandomValues works on the existing LAN flow.
	const bytes = crypto.getRandomValues(new Uint8Array(16));
	bytes[6] = (bytes[6]! & 15) | 64;
	bytes[8] = (bytes[8]! & 63) | 128;
	const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(
		"",
	);
	return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
