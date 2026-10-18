import { invoke } from "@tauri-apps/api/core";

/** "Fechar" on a finished goal: the session forgets it. */
export const dismissSessionGoal = (sessionId: string) =>
	invoke<void>("session_dismiss_goal", { sessionId });
