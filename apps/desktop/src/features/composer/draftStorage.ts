export function loadDraft(key: string) {
	if (typeof window === "undefined") {
		return "";
	}

	return window.localStorage.getItem(key) ?? "";
}

export function saveDraft(key: string, value: string) {
	if (typeof window === "undefined") {
		return;
	}

	if (value.trim().length === 0) {
		window.localStorage.removeItem(key);
		return;
	}

	window.localStorage.setItem(key, value);
}

export function clearDraft(key: string) {
	if (typeof window === "undefined") {
		return;
	}

	window.localStorage.removeItem(key);
}
