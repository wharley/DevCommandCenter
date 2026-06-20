const OPEN_PREFERRED_EDITOR_KEY = "o";
const QUICK_OPEN_KEY = "p";

function isMacPlatform(platform: string) {
	return /mac/i.test(platform);
}

export function getOpenPreferredEditorShortcutKeys(platform = navigator.platform) {
	return isMacPlatform(platform) ? ["Cmd", "O"] : ["Ctrl", "O"];
}

export function getQuickOpenShortcutKeys(platform = navigator.platform) {
	return isMacPlatform(platform) ? ["Cmd", "P"] : ["Ctrl", "P"];
}

/** Quick Open (Cmd/Ctrl+P): open a file by name without leaving the DCC. */
export function isQuickOpenShortcut(
	event: Pick<
		KeyboardEvent,
		"key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "defaultPrevented"
	>,
	platform = navigator.platform,
) {
	if (event.defaultPrevented) {
		return false;
	}

	const key = event.key.toLowerCase();
	if (key !== QUICK_OPEN_KEY || event.altKey || event.shiftKey) {
		return false;
	}

	if (isMacPlatform(platform)) {
		return event.metaKey && !event.ctrlKey;
	}

	return event.ctrlKey && !event.metaKey;
}

export function isOpenPreferredEditorShortcut(
	event: Pick<
		KeyboardEvent,
		"key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "defaultPrevented"
	>,
	platform = navigator.platform,
) {
	if (event.defaultPrevented) {
		return false;
	}

	const key = event.key.toLowerCase();
	if (key !== OPEN_PREFERRED_EDITOR_KEY || event.altKey || event.shiftKey) {
		return false;
	}

	if (isMacPlatform(platform)) {
		return event.metaKey && !event.ctrlKey;
	}

	return event.ctrlKey && !event.metaKey;
}

export function shouldIgnoreGlobalShortcutTarget(target: EventTarget | null) {
	if (!(target instanceof HTMLElement)) {
		return false;
	}

	if (target.isContentEditable) {
		return true;
	}

	return Boolean(target.closest("input, textarea, select, [contenteditable='true'], [role='textbox']"));
}
