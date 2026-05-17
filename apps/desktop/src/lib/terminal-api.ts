import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type TerminalSpawnOptions = {
	cwd: string;
	command?: string;
	args?: string[];
	cols?: number;
	rows?: number;
	paneId?: string | null;
	ptyOwnerKey?: string | null;
	restart?: boolean;
};

export type TerminalSpawnResult = {
	ptyId: string;
};

export type TerminalSessionResult = {
	ptyId: string;
	cwd: string;
	command: string;
	args: string[];
	ptyOwnerKey?: string | null;
	status: string;
	startedAt: string;
	exitedAt?: string | null;
	lastExitCode?: number | null;
};

export type TerminalEnsureResult = {
	ptyId: string;
	existing: boolean;
	session: TerminalSessionResult;
	chunks: string[];
	truncated: boolean;
};

export type TerminalOutputEvent = {
	ptyId: string;
	data: string;
	stream: string;
};

export type TerminalExitEvent = {
	ptyId: string;
	code: number | null;
};

export type ShellDefaultResult = {
	shell: string;
};

const TERMINAL_OUTPUT_EVENT = "terminal-output";
const TERMINAL_EXIT_EVENT = "terminal-exit";

/**
 * Stable scope key for terminal state caching. Now that we only talk to the
 * local daemon there is only one possible scope, but callers still pass this
 * through React keys so we keep it as a constant.
 */
export function getTerminalBackendScope() {
	return "local";
}

export function getDefaultShell() {
	return invoke<ShellDefaultResult>("shell_get_default");
}

export function spawnTerminal(options: TerminalSpawnOptions) {
	return invoke<TerminalSpawnResult>("terminal_spawn", { options });
}

export async function getOrCreateTerminalByOwner(
	ownerKey: string,
	options: TerminalSpawnOptions,
): Promise<TerminalEnsureResult> {
	const payload = await invoke<{
		ptyId: string;
		session?: TerminalSessionResult | null;
	}>("terminal_get_or_create", {
		missionId: ownerKey,
		options: {
			...options,
			ptyOwnerKey: ownerKey,
		},
	});

	return {
		ptyId: payload.ptyId,
		existing: false,
		session:
			payload.session ?? {
				ptyId: payload.ptyId,
				cwd: options.cwd,
				command: options.command ?? "",
				args: options.args ?? [],
				ptyOwnerKey: ownerKey,
				status: "running",
				startedAt: new Date().toISOString(),
				exitedAt: null,
				lastExitCode: null,
			},
		chunks: [],
		truncated: false,
	};
}

export function writeTerminalStdin(ptyId: string, data: string) {
	return invoke<{ ok: boolean }>("terminal_write", { ptyId, data });
}

export function resizeTerminal(ptyId: string, cols: number, rows: number) {
	return invoke<{ ok: boolean }>("terminal_resize", { ptyId, cols, rows });
}

export function killTerminal(ptyId: string) {
	return invoke<{ ok: boolean }>("terminal_kill", { ptyId });
}

export async function listenTerminalOutput(
	handler: (event: TerminalOutputEvent) => void,
): Promise<UnlistenFn> {
	return listen<TerminalOutputEvent>(TERMINAL_OUTPUT_EVENT, (event) => {
		handler(event.payload);
	});
}

export async function listenTerminalExit(
	handler: (event: TerminalExitEvent) => void,
): Promise<UnlistenFn> {
	return listen<TerminalExitEvent>(TERMINAL_EXIT_EVENT, (event) => {
		handler(event.payload);
	});
}
