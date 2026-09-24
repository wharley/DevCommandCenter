import { invoke } from "@tauri-apps/api/core";
import type {
	ProviderRuntimeConfig,
	WorkspaceIsolationMode,
} from "@dcc/contracts";
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";

export type QuickRequest = {
	projectId: string;
	rootPath: string;
	baseBranch: string;
	isolationMode: WorkspaceIsolationMode;
	providerId: string;
	modelId: string;
	providerRuntime: ProviderRuntimeConfig | null;
	turn: ComposerSubmittedTurn;
};
export type LaunchPhase =
	| "pending"
	| "creating"
	| "workspaceReady"
	| "starting"
	| "sessionReady"
	| "sending"
	| "completed"
	| "failed";
export type QuickLaunch = {
	id: string;
	revision: number;
	phase: LaunchPhase;
	request: QuickRequest;
	workspaceId: string | null;
	sessionId: string | null;
	error: string | null;
};
export type QuickStatus = {
	supported: boolean;
	shortcut: string | null;
	shortcutError: boolean;
	launch: QuickLaunch | null;
};
export const quickStatus = () => invoke<QuickStatus>("quick_composer_status");
export const toggleQuickComposer = () => invoke<void>("quick_composer_toggle");
export const hideQuickComposer = () => invoke<void>("quick_composer_hide");
export const openQuickTask = () => invoke<void>("quick_composer_open_main");
export const setQuickShortcut = (shortcut: string | null) =>
	invoke<void>("quick_composer_set_shortcut", { shortcut });
export const beginQuickLaunch = (launch: QuickLaunch) =>
	invoke<QuickLaunch>("quick_composer_begin", { launch });
export const checkpointQuickLaunch = (launch: QuickLaunch) =>
	invoke<QuickLaunch>("quick_composer_checkpoint", { launch });

export function newQuickLaunch(request: QuickRequest): QuickLaunch {
	return {
		id: crypto.randomUUID(),
		revision: 0,
		phase: "pending",
		request,
		workspaceId: null,
		sessionId: null,
		error: null,
	};
}
