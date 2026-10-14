import { invoke } from "@tauri-apps/api/core";

/** The global "Continuar sozinho se o limite acabar" preference (off by default). */
export type UsageLimitSettings = { autoResume: boolean };

export const readUsageLimitSettings = () => invoke<UsageLimitSettings>("usage_limit_settings");
export const saveUsageLimitSettings = (settings: UsageLimitSettings) =>
	invoke<UsageLimitSettings>("usage_limit_set_settings", { settings });

/** Whether this task continues by itself after a limit (its choice, else the global one). */
export const readSessionAutoResume = (sessionId: string) =>
	invoke<boolean>("session_usage_limit_auto_resume", { sessionId });
export const saveSessionAutoResume = (sessionId: string, enabled: boolean) =>
	invoke<void>("session_set_usage_limit_auto_resume", { sessionId, enabled });

/** "Continuar" / "Tentar de novo": sends the fixed continuation turn now. */
export const resumeAfterUsageLimit = (sessionId: string) =>
	invoke<string>("session_resume_after_usage_limit", { sessionId });

/** "Continuar com a conta …": moves the task to that account (`null` = system login) and continues. */
export const resumeAfterUsageLimitOnAccount = (sessionId: string, accountId: string | null) =>
	invoke<string>("session_resume_after_usage_limit_on_account", { sessionId, accountId });

export const SESSION_AUTO_RESUME_QUERY_KEY = "session-auto-resume";
export const USAGE_LIMIT_SETTINGS_QUERY_KEY = "usage-limit-settings";
