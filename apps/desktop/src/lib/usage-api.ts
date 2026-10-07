import { invoke } from "@tauri-apps/api/core";
import { SESSION_METHODS } from "@dcc/contracts";
import type { SessionTurnUsage, UsageDashboard, UsageDashboardInput } from "@dcc/contracts";

export function loadUsageDashboard(input: UsageDashboardInput) {
	return invoke<UsageDashboard>(SESSION_METHODS.usageDashboard, { input });
}

export function loadSessionTurnUsage(sessionId: string) {
	return invoke<SessionTurnUsage[]>(SESSION_METHODS.sessionTurnUsage, { sessionId });
}
