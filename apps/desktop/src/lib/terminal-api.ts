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

export function getDefaultShell() {
	return invoke<ShellDefaultResult>("shell_get_default");
}

export function spawnTerminal(options: TerminalSpawnOptions) {
	return invoke<TerminalSpawnResult>("terminal_spawn", { options });
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
