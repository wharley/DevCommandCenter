import { useSyncExternalStore } from "react";

export const CODERABBIT_INTEGRATION_ENABLED_STORAGE_KEY =
	"dcc.settings.coderabbit.integration-enabled";

const CODERABBIT_INTEGRATION_CHANGED_EVENT =
	"dcc:coderabbit-integration-changed";

export function getCodeRabbitIntegrationEnabled(): boolean {
	if (typeof window === "undefined") {
		return true;
	}

	try {
		return (
			window.localStorage.getItem(
				CODERABBIT_INTEGRATION_ENABLED_STORAGE_KEY,
			) !== "false"
		);
	} catch {
		return true;
	}
}

export function setCodeRabbitIntegrationEnabled(enabled: boolean): void {
	if (typeof window === "undefined") {
		return;
	}

	try {
		window.localStorage.setItem(
			CODERABBIT_INTEGRATION_ENABLED_STORAGE_KEY,
			String(enabled),
		);
	} catch {
		// Keep the current window in sync even when persistence is unavailable.
	}

	window.dispatchEvent(new Event(CODERABBIT_INTEGRATION_CHANGED_EVENT));
}

function subscribeCodeRabbitIntegration(listener: () => void): () => void {
	if (typeof window === "undefined") {
		return () => undefined;
	}

	const handleStorage = (event: StorageEvent) => {
		if (event.key === CODERABBIT_INTEGRATION_ENABLED_STORAGE_KEY) {
			listener();
		}
	};

	window.addEventListener(CODERABBIT_INTEGRATION_CHANGED_EVENT, listener);
	window.addEventListener("storage", handleStorage);

	return () => {
		window.removeEventListener(CODERABBIT_INTEGRATION_CHANGED_EVENT, listener);
		window.removeEventListener("storage", handleStorage);
	};
}

export function useCodeRabbitIntegrationEnabled(): boolean {
	return useSyncExternalStore(
		subscribeCodeRabbitIntegration,
		getCodeRabbitIntegrationEnabled,
		() => true,
	);
}
