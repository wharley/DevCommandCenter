import { afterEach, describe, expect, it, vi } from "vitest";
import {
	CODERABBIT_INTEGRATION_ENABLED_STORAGE_KEY,
	getCodeRabbitIntegrationEnabled,
	setCodeRabbitIntegrationEnabled,
} from "./coderabbit-preferences";

function createStorage(initial: Record<string, string> = {}): Storage {
	const values = new Map(Object.entries(initial));
	return {
		get length() {
			return values.size;
		},
		clear: () => values.clear(),
		getItem: (key) => values.get(key) ?? null,
		key: (index) => Array.from(values.keys())[index] ?? null,
		removeItem: (key) => values.delete(key),
		setItem: (key, value) => values.set(key, value),
	};
}

afterEach(() => {
	vi.unstubAllGlobals();
});

describe("CodeRabbit integration preference", () => {
	it("keeps the existing integration enabled until the user disables it", () => {
		vi.stubGlobal("window", {
			localStorage: createStorage(),
		});

		expect(getCodeRabbitIntegrationEnabled()).toBe(true);
	});

	it("persists disabled and enabled states independently from CLI auth", () => {
		const localStorage = createStorage();
		vi.stubGlobal("window", {
			localStorage,
			dispatchEvent: vi.fn(),
		});

		setCodeRabbitIntegrationEnabled(false);
		expect(
			localStorage.getItem(CODERABBIT_INTEGRATION_ENABLED_STORAGE_KEY),
		).toBe("false");
		expect(getCodeRabbitIntegrationEnabled()).toBe(false);

		setCodeRabbitIntegrationEnabled(true);
		expect(getCodeRabbitIntegrationEnabled()).toBe(true);
	});
});
