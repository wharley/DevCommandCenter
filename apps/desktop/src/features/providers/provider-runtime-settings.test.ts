import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	PROVIDER_RUNTIME_STORAGE_KEY,
	draftToProviderRuntimeConfig,
	getProviderRuntimeDraft,
	readProviderRuntimeSettings,
	setProviderRuntimeDraft,
	supportsProviderRuntime,
	supportsProviderSubagentConcurrency,
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
			binaryPath: "",
			homePath: "/tmp/codex",
			shadowHomePath: "",
			maxConcurrentSubagents: "",
		});
	});

	it("projects a supported concurrency limit into the session runtime", () => {
		expect(
			draftToProviderRuntimeConfig({
				binaryPath: "",
				homePath: "",
				shadowHomePath: "",
				maxConcurrentSubagents: "4",
			}),
		).toEqual({
			binaryPath: null,
			homePath: null,
			shadowHomePath: null,
			maxConcurrentSubagents: 4,
		});
	});

	it("drops empty settings but keeps a concurrency-only preference", () => {
		const concurrencyOnly = setProviderRuntimeDraft({}, "codex", {
			binaryPath: "",
			homePath: "",
			shadowHomePath: "",
			maxConcurrentSubagents: "2",
		});
		expect(concurrencyOnly.codex?.maxConcurrentSubagents).toBe("2");

		expect(
			setProviderRuntimeDraft(concurrencyOnly, "codex", {
				binaryPath: "",
				homePath: "",
				shadowHomePath: "",
				maxConcurrentSubagents: "",
			}),
		).toEqual({});
	});

	it("projects only the runtime fields the provider capability declares", () => {
		const draft = {
			binaryPath: "/opt/dcc/provider",
			homePath: "~/dcc-home",
			shadowHomePath: "~/dcc-shadow",
			maxConcurrentSubagents: "4",
		};

		expect(
			draftToProviderRuntimeConfig(draft, {
				supportsRuntimeBinary: true,
				supportsRuntimeHome: true,
				supportsShadowHome: true,
				supportsSubagentConcurrency: true,
			}),
		).toEqual({
			binaryPath: "/opt/dcc/provider",
			homePath: "~/dcc-home",
			shadowHomePath: "~/dcc-shadow",
			maxConcurrentSubagents: 4,
		});

		expect(
			draftToProviderRuntimeConfig(draft, {
				supportsRuntimeBinary: false,
				supportsRuntimeHome: true,
				supportsShadowHome: false,
				supportsSubagentConcurrency: false,
			}),
		).toEqual({
			binaryPath: null,
			homePath: "~/dcc-home",
			shadowHomePath: null,
			maxConcurrentSubagents: null,
		});

		// Stale local drafts for an adapter that honors nothing never reach the backend.
		expect(
			draftToProviderRuntimeConfig(draft, {
				supportsRuntimeHome: false,
				supportsShadowHome: false,
				supportsSubagentConcurrency: false,
			}),
		).toBeNull();
		// Legacy catalogs without the fields behave as unsupported.
		expect(draftToProviderRuntimeConfig(draft, {})).toBeNull();
	});

	it("derives runtime settings visibility from backend capabilities", () => {
		expect(supportsProviderRuntime({ supportsRuntimeHome: true })).toBe(true);
		expect(supportsProviderRuntime({ supportsSubagentConcurrency: true })).toBe(
			true,
		);
		expect(supportsProviderRuntime({})).toBe(false);
		expect(supportsProviderRuntime(null)).toBe(false);
		expect(
			supportsProviderSubagentConcurrency({ supportsSubagentConcurrency: true }),
		).toBe(true);
		expect(supportsProviderSubagentConcurrency({ supportsRuntimeHome: true })).toBe(
			false,
		);
	});

	it("ignores unsupported persisted limits", () => {
		window.localStorage.setItem(
			PROVIDER_RUNTIME_STORAGE_KEY,
			JSON.stringify({ codex: { maxConcurrentSubagents: "999" } }),
		);

		expect(getProviderRuntimeDraft(readProviderRuntimeSettings(), "codex")).toEqual({
			binaryPath: "",
			homePath: "",
			shadowHomePath: "",
			maxConcurrentSubagents: "",
		});
	});
});
