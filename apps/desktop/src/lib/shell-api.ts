import { invoke } from "@tauri-apps/api/core";

export function openExternal(url: string) {
	return invoke<{ ok: boolean }>("shell_open_external", { url });
}

export function openPath(path: string) {
	return invoke<{ ok: boolean }>("shell_open_path", { path });
}

export function openInEditor(path: string, editor: string) {
	return invoke<{ success: boolean; error?: string }>("shell_open_in_editor", {
		path,
		editor,
	});
}

export function openTerminalAtPath(
	dirPath: string,
	suggestedCommand?: unknown,
) {
	return invoke<{ success: boolean; error?: string }>("shell_open_terminal_at_path", {
		dirPath,
		suggestedCommand,
	});
}
