import { invoke } from "@tauri-apps/api/core";
import {
	getActiveRemoteEnvironment,
	type SavedRemoteEnvironment,
} from "@/features/settings/remote-environments-store";

type JsonValue =
	| null
	| boolean
	| number
	| string
	| JsonValue[]
	| { [key: string]: JsonValue };

type RpcResponse<T> = {
	ok: boolean;
	result?: T;
	error?: string | null;
};

export type DaemonCombStatus =
	| "active"
	| "ready_for_review"
	| "applied"
	| "discarded"
	| "archived"
	| "error";

export type DaemonComb = {
	id: string;
	projectId: string | null;
	name: string | null;
	description: string | null;
	baseBranch: string | null;
	branch: string | null;
	worktreePath: string | null;
	status: DaemonCombStatus | null;
	lastOpenedAt: string | null;
	lastGitActivityAt: string | null;
	isPinned?: boolean | null;
};

export type BackendTarget =
	| { kind: "local" }
	| { kind: "remote"; environment: SavedRemoteEnvironment };

export function getCurrentBackendTarget(): BackendTarget {
	const environment = getActiveRemoteEnvironment();
	if (environment?.endpoint && environment.bearerToken) {
		return { kind: "remote", environment };
	}
	return { kind: "local" };
}

async function remoteRpc<T>(
	environment: SavedRemoteEnvironment,
	method: string,
	params: JsonValue,
): Promise<T> {
	const endpoint = environment.endpoint?.trim();
	const bearerToken = environment.bearerToken?.trim();
	if (!endpoint || !bearerToken) {
		throw new Error("Remote environment is missing endpoint or bearer token.");
	}

	const url = new URL("/rpc", endpoint.endsWith("/") ? endpoint : `${endpoint}/`);
	const response = await fetch(url, {
		method: "POST",
		headers: {
			Authorization: `Bearer ${bearerToken}`,
			"Content-Type": "application/json",
		},
		body: JSON.stringify({ method, params }),
	});

	let payload: RpcResponse<T> | null = null;
	try {
		payload = (await response.json()) as RpcResponse<T>;
	} catch {
		payload = null;
	}

	if (!response.ok) {
		throw new Error(payload?.error || `Remote backend returned HTTP ${response.status}`);
	}
	if (!payload?.ok) {
		throw new Error(payload?.error || `Remote RPC failed for ${method}`);
	}
	return payload.result as T;
}

async function daemonCall<T>(
	localCommand: string,
	method: string,
	params: JsonValue,
	environment?: SavedRemoteEnvironment | null,
): Promise<T> {
	const target = environment ? { kind: "remote" as const, environment } : getCurrentBackendTarget();
	if (target.kind === "remote") {
		return remoteRpc<T>(target.environment, method, params);
	}

	if (params && typeof params === "object" && !Array.isArray(params)) {
		return invoke<T>(localCommand, params as Record<string, unknown>);
	}
	return invoke<T>(localCommand);
}

export function daemonHealth(environment?: SavedRemoteEnvironment | null) {
	return daemonCall<unknown>("daemon_health", "daemon.health", {}, environment);
}

export function daemonGetStatus(environment?: SavedRemoteEnvironment | null) {
	return daemonCall<unknown>("daemon_get_status", "daemon.getStatus", {}, environment);
}

export function daemonListTasks(environment?: SavedRemoteEnvironment | null) {
	return daemonCall<unknown>("daemon_list_tasks", "daemon.listTasks", {}, environment);
}

export function daemonListProcesses(
	projectId?: string | null,
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<unknown>(
		"daemon_list_processes",
		"daemon.listProcesses",
		{ projectId: projectId ?? null },
		environment,
	);
}

export function daemonStartProcess(
	projectId: string,
	processId: string,
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<unknown>(
		"daemon_start_process",
		"daemon.startProcess",
		{ projectId, processId },
		environment,
	);
}

export function daemonStopProcess(
	projectId: string,
	processId: string,
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<unknown>(
		"daemon_stop_process",
		"daemon.stopProcess",
		{ projectId, processId },
		environment,
	);
}

export function daemonRestartProcess(
	projectId: string,
	processId: string,
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<unknown>(
		"daemon_restart_process",
		"daemon.restartProcess",
		{ projectId, processId },
		environment,
	);
}

export function daemonListCombs(
	projectId?: string | null,
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<DaemonComb[]>(
		"daemon_list_combs",
		"combs.list",
		{ projectId: projectId ?? null },
		environment,
	);
}

export function daemonListPanes(
	input?: { projectId?: string | null; combId?: string | null },
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<unknown>(
		"daemon_list_panes",
		"panes.list",
		{
			projectId: input?.projectId ?? null,
			combId: input?.combId ?? null,
		},
		environment,
	);
}

export function daemonGetDiffsBundle(
	input?: { worktreePaths?: string[]; combIds?: string[] },
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<unknown>(
		"daemon_get_diffs_bundle",
		"diffs.bundle",
		{
			worktreePaths: input?.worktreePaths ?? [],
			combIds: input?.combIds ?? [],
		},
		environment,
	);
}

export function daemonRunTask(
	projectId: string,
	taskId: string,
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<unknown>(
		"daemon_run_task",
		"daemon.runTask",
		{ projectId, taskId },
		environment,
	);
}

export function daemonAttachTask(
	projectId: string,
	taskId: string,
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<unknown>(
		"daemon_attach_task",
		"daemon.attachTask",
		{ projectId, taskId },
		environment,
	);
}

export function daemonDetachTask(
	projectId: string,
	taskId: string,
	environment?: SavedRemoteEnvironment | null,
) {
	return daemonCall<unknown>(
		"daemon_detach_task",
		"daemon.detachTask",
		{ projectId, taskId },
		environment,
	);
}
