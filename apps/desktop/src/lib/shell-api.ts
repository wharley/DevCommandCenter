import { invoke } from "@tauri-apps/api/core";

export function openExternal(url: string) {
	return invoke<{ ok: boolean }>("shell_open_external", { url });
}

export function openPath(path: string) {
	return invoke<{ ok: boolean }>("shell_open_path", { path });
}
