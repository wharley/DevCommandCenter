import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	PROVIDER_RUNTIME_STORAGE_KEY,
	draftToProviderRuntimeConfig,
	getProviderRuntimeDraft,
	readProviderRuntimeSettings,
	setProviderRuntimeDraft,
} from "./provider-runtime-settings";

function createStorage() {
	const values = new Map<string, string>();
	return {
		getItem: (key: string) => values.get(key) ?? null,
		setItem: (key: string, value: string) => values.set(key, value),
		removeItem: (key: string) => values.delete(key),
	};
}

describe("provider runtime settings", () => {
	beforeEach(() => {
		vi.stubGlobal("window", { localStorage: createStorage() });
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("keeps legacy home settings compatible", () => {
		window.localStorage.setItem(
			PROVIDER_RUNTIME_STORAGE_KEY,
			JSON.stringify({ codex: { homePath: "/tmp/codex", shadowHomePath: "" } }),
		);

		expect(getProviderRuntimeDraft(readProviderRuntimeSettings(), "codex")).toEqual({
			homePath: "/tmp/codex",
			shadowHomePath: "",
			maxConcurrentSubagents: "",
		});
	});

	it("projects a supported concurrency limit into the session runtime", () => {
		expect(
			draftToProviderRuntimeConfig({
				homePath: "",
				shadowHomePath: "",
				maxConcurrentSubagents: "4",
			}),
		).toEqual({
			homePath: null,
			shadowHomePath: null,
			maxConcurrentSubagents: 4,
		});
	});

	it("drops empty settings but keeps a concurrency-only preference", () => {
		const concurrencyOnly = setProviderRuntimeDraft({}, "codex", {
			homePath: "",
			shadowHomePath: "",
			maxConcurrentSubagents: "2",
		});
		expect(concurrencyOnly.codex?.maxConcurrentSubagents).toBe("2");

		expect(
			setProviderRuntimeDraft(concurrencyOnly, "codex", {
				homePath: "",
				shadowHomePath: "",
				maxConcurrentSubagents: "",
			}),
		).toEqual({});
	});

	it("ignores unsupported persisted limits", () => {
		window.localStorage.setItem(
			PROVIDER_RUNTIME_STORAGE_KEY,
			JSON.stringify({ codex: { maxConcurrentSubagents: "999" } }),
		);

		expect(getProviderRuntimeDraft(readProviderRuntimeSettings(), "codex")).toEqual({
			homePath: "",
			shadowHomePath: "",
			maxConcurrentSubagents: "",
		});
	});
});
