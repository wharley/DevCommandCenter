import { invoke } from "@tauri-apps/api/core";
import type { ComputerUseTarget } from "./computer-use-api";

export type AppshotStatus = {
	supported: boolean;
	screenRecording: boolean;
	shortcut: string | null;
	shortcutError: boolean;
	targets: ComputerUseTarget[];
};
export type AppshotPreview = { id: string; dataUrl: string };
export type PendingAppshot = { id: string; draftKey: string; path: string };
export const isAppshotsDesktop = () =>
	typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function getAppshotStatus(
	includeTargets = false,
): Promise<AppshotStatus> {
	if (!isAppshotsDesktop())
		return Promise.resolve({
			supported: false,
			screenRecording: false,
			shortcut: null,
			shortcutError: false,
			targets: [],
		});
	return invoke<AppshotStatus>("appshots_status", { includeTargets }).catch(
		(error: unknown) => {
			const message = error instanceof Error ? error.message : String(error);
			if (/command\s+appshots_status\s+not\s+found/i.test(message)) {
				throw new Error("backendOutdated");
			}
			throw error;
		},
	);
}
export const requestAppshotAccess = () =>
	invoke<void>("appshots_request_access");
export const previewAppshot = (draftKey: string, target: ComputerUseTarget) =>
	invoke<AppshotPreview>("appshots_preview", { draftKey, target });
export const attachAppshots = (draftKey: string, ids: string[]) =>
	invoke<void>("appshots_attach", { draftKey, ids });
export const activateAppshots = (owner: string, draftKey: string | null) =>
	invoke<void>("appshots_activate", { owner, draftKey });
export const pendingAppshots = (draftKey: string) =>
	invoke<PendingAppshot[]>("appshots_pending", { draftKey });
export const acknowledgeAppshots = (draftKey: string, ids: string[]) =>
	invoke<void>("appshots_acknowledge", { draftKey, ids });
export const setAppshotShortcut = (shortcut: string | null) =>
	invoke<void>("appshots_set_shortcut", { shortcut });

const ERROR_CODES = new Set([
	"unsupported",
	"backendOutdated",
	"permission",
	"unavailable",
	"captureFailed",
	"windowUnavailable",
	"saveFailed",
	"selectionInvalid",
	"previewExpired",
	"shortcutInvalid",
	"shortcutUnavailable",
]);
export function appshotErrorKey(error: unknown) {
	const code =
		typeof error === "string"
			? error
			: error instanceof Error
				? error.message
				: "unavailable";
	return `appshots.errors.${ERROR_CODES.has(code) ? code : "unavailable"}`;
}

export function shortcutFromEvent(
	event: Pick<
		KeyboardEvent,
		"code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"
	>,
): string | null {
	if (!event.metaKey && !event.ctrlKey && !event.altKey) return null;
	if (!/^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Space)$/.test(event.code))
		return null;
	return [
		event.metaKey && "Super",
		event.ctrlKey && "Control",
		event.altKey && "Alt",
		event.shiftKey && "Shift",
		event.code,
	]
		.filter(Boolean)
		.join("+");
}

export const appshotTargetKey = (target: ComputerUseTarget) =>
	`${target.pid}:${target.windowId}`;
