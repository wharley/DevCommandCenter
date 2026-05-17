import { invoke } from "@tauri-apps/api/core";

type JsonValue =
	| null
	| boolean
	| number
	| string
	| JsonValue[]
	| { [key: string]: JsonValue };

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

async function daemonCall<T>(localCommand: string, params: JsonValue): Promise<T> {
	if (params && typeof params === "object" && !Array.isArray(params)) {
		return invoke<T>(localCommand, params as Record<string, unknown>);
	}
	return invoke<T>(localCommand);
}

export function daemonHealth() {
	return daemonCall<unknown>("daemon_health", {});
}

export function daemonGetStatus() {
	return daemonCall<unknown>("daemon_get_status", {});
}

export function daemonListTasks() {
	return daemonCall<unknown>("daemon_list_tasks", {});
}

export function daemonListProcesses(projectId?: string | null) {
	return daemonCall<unknown>("daemon_list_processes", {
		projectId: projectId ?? null,
	});
}

export function daemonStartProcess(projectId: string, processId: string) {
	return daemonCall<unknown>("daemon_start_process", { projectId, processId });
}

export function daemonStopProcess(projectId: string, processId: string) {
	return daemonCall<unknown>("daemon_stop_process", { projectId, processId });
}

export function daemonRestartProcess(projectId: string, processId: string) {
	return daemonCall<unknown>("daemon_restart_process", { projectId, processId });
}

export function daemonListCombs(projectId?: string | null) {
	return daemonCall<DaemonComb[]>("daemon_list_combs", {
		projectId: projectId ?? null,
	});
}

export function daemonListPanes(input?: {
	projectId?: string | null;
	combId?: string | null;
}) {
	return daemonCall<unknown>("daemon_list_panes", {
		projectId: input?.projectId ?? null,
		combId: input?.combId ?? null,
	});
}

export function daemonGetDiffsBundle(input?: {
	worktreePaths?: string[];
	combIds?: string[];
}) {
	return daemonCall<unknown>("daemon_get_diffs_bundle", {
		worktreePaths: input?.worktreePaths ?? [],
		combIds: input?.combIds ?? [],
	});
}

export function daemonRunTask(projectId: string, taskId: string) {
	return daemonCall<unknown>("daemon_run_task", { projectId, taskId });
}

export function daemonAttachTask(projectId: string, taskId: string) {
	return daemonCall<unknown>("daemon_attach_task", { projectId, taskId });
}

export function daemonDetachTask(projectId: string, taskId: string) {
	return daemonCall<unknown>("daemon_detach_task", { projectId, taskId });
}
