import { invoke } from "@tauri-apps/api/core";
import { SESSION_METHODS } from "@dcc/contracts";
import type { UsageDashboard, UsageDashboardInput } from "@dcc/contracts";

export function loadUsageDashboard(input: UsageDashboardInput) {
	return invoke<UsageDashboard>(SESSION_METHODS.usageDashboard, { input });
}
