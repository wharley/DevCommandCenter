const OPEN_PREFERRED_EDITOR_KEY = "o";
const QUICK_OPEN_KEY = "p";
const WORKSPACE_SEARCH_KEY = "f";
const INSPECTOR_GIT_MODE_KEY = "g";
const INSPECTOR_CODE_MODE_KEY = "e";
const COMMAND_PALETTE_KEY = "k";
const LEGACY_COMMAND_PALETTE_KEY = "p";
const TOGGLE_TERMINAL_KEY = "j";
const FOCUS_COMPOSER_KEY = "l";

function isMacPlatform(platform: string) {
	return /mac/i.test(platform);
}

export function getPrimaryShortcutModifier(platform = navigator.platform) {
	return isMacPlatform(platform) ? "Cmd" : "Ctrl";
}

export function getCommandPaletteShortcutKeys(platform = navigator.platform) {
	return [getPrimaryShortcutModifier(platform), "K"];
}

export function getLegacyCommandPaletteShortcutKeys(platform = navigator.platform) {
	return [getPrimaryShortcutModifier(platform), "Shift", "P"];
}

export function getToggleTerminalShortcutKeys(platform = navigator.platform) {
	return [getPrimaryShortcutModifier(platform), "J"];
}

export function getFocusComposerShortcutKeys(platform = navigator.platform) {
	return [getPrimaryShortcutModifier(platform), "Shift", "L"];
}

export function getOpenPreferredEditorShortcutKeys(platform = navigator.platform) {
	return isMacPlatform(platform) ? ["Cmd", "O"] : ["Ctrl", "O"];
}

export function getQuickOpenShortcutKeys(platform = navigator.platform) {
	return isMacPlatform(platform) ? ["Cmd", "P"] : ["Ctrl", "P"];
}

export function getWorkspaceSearchShortcutKeys(platform = navigator.platform) {
	return isMacPlatform(platform)
		? ["Cmd", "Shift", "F"]
		: ["Ctrl", "Shift", "F"];
}

export function getInspectorGitModeShortcutKeys(platform = navigator.platform) {
	return isMacPlatform(platform)
		? ["Cmd", "Shift", "G"]
		: ["Ctrl", "Shift", "G"];
}

export function getInspectorCodeModeShortcutKeys(platform = navigator.platform) {
	return isMacPlatform(platform)
		? ["Cmd", "Shift", "E"]
		: ["Ctrl", "Shift", "E"];
}

function isModifiedShiftShortcut(
	event: Pick<
		KeyboardEvent,
		"key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "defaultPrevented"
	>,
	key: string,
	platform = navigator.platform,
) {
	if (event.defaultPrevented) {
		return false;
	}
	if (event.key.toLowerCase() !== key || event.altKey || !event.shiftKey) {
		return false;
	}
	if (isMacPlatform(platform)) {
		return event.metaKey && !event.ctrlKey;
	}
	return event.ctrlKey && !event.metaKey;
}

function isModifiedShortcut(
	event: Pick<
		KeyboardEvent,
		"key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "defaultPrevented"
	>,
	key: string,
	shiftKey: boolean,
	platform = navigator.platform,
) {
	if (
		event.defaultPrevented ||
		event.key.toLowerCase() !== key ||
		event.altKey ||
		event.shiftKey !== shiftKey
	) {
		return false;
	}
	return isMacPlatform(platform)
		? event.metaKey && !event.ctrlKey
		: event.ctrlKey && !event.metaKey;
}

export function isCommandPaletteShortcut(
	event: Parameters<typeof isModifiedShortcut>[0],
	platform = navigator.platform,
) {
	return (
		isModifiedShortcut(event, COMMAND_PALETTE_KEY, false, platform) ||
		isModifiedShortcut(event, LEGACY_COMMAND_PALETTE_KEY, true, platform)
	);
}

export function isToggleTerminalShortcut(
	event: Parameters<typeof isModifiedShortcut>[0],
	platform = navigator.platform,
) {
	return isModifiedShortcut(event, TOGGLE_TERMINAL_KEY, false, platform);
}

export function isFocusComposerShortcut(
	event: Parameters<typeof isModifiedShortcut>[0],
	platform = navigator.platform,
) {
	return isModifiedShortcut(event, FOCUS_COMPOSER_KEY, true, platform);
}

export function isInspectorGitModeShortcut(
	event: Pick<
		KeyboardEvent,
		"key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "defaultPrevented"
	>,
	platform = navigator.platform,
) {
	return isModifiedShiftShortcut(event, INSPECTOR_GIT_MODE_KEY, platform);
}

export function isInspectorCodeModeShortcut(
	event: Pick<
		KeyboardEvent,
		"key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "defaultPrevented"
	>,
	platform = navigator.platform,
) {
	return isModifiedShiftShortcut(event, INSPECTOR_CODE_MODE_KEY, platform);
}

/** Workspace search (Cmd/Ctrl+Shift+F): find text across the worktree. */
export function isWorkspaceSearchShortcut(
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
	if (key !== WORKSPACE_SEARCH_KEY || event.altKey || !event.shiftKey) {
		return false;
	}

	if (isMacPlatform(platform)) {
		return event.metaKey && !event.ctrlKey;
	}

	return event.ctrlKey && !event.metaKey;
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
