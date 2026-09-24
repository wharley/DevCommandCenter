import { invoke } from "@tauri-apps/api/core";
export type MenuBarTask = {
	workspaceId: string;
	sessionId: string;
	title: string;
	project: string;
	status: "running" | "permission" | "input" | "completed" | "aborted";
	updatedAt: string;
};
export type MenuBarSnapshot = {
	tasks: MenuBarTask[];
	metrics: { cpuPercent: number | null; memoryBytes: number; processCount: number } | null;
};
export const readMenuBar = () => invoke<MenuBarSnapshot>("menu_bar_snapshot");
export const isMenuBarVisible = () => invoke<boolean>("menu_bar_visible");
export const hideMenuBar = () => invoke<void>("menu_bar_hide");
export const composeFromMenuBar = () => invoke<void>("menu_bar_compose");
export const quitFromMenuBar = () => invoke<void>("menu_bar_quit");
export const openFromMenuBar = (task?: MenuBarTask) => invoke<void>("menu_bar_open_main", {
	workspaceId: task?.workspaceId ?? null,
	sessionId: task?.sessionId ?? null,
});
