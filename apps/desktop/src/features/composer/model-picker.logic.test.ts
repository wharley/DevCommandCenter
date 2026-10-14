import { describe, expect, it } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import {
	buildModelPickerRows,
	effortSliderSteps,
	FAVORITES_TAB,
	initialModelPickerTab,
	pickerProviders,
	providersNeedingSetup,
	shortcutRowIndex,
} from "./model-picker.logic";

const providers = FALLBACK_PROVIDER_CATALOG.providers;
const withCodexOff = providers.map((provider) =>
	provider.id === "codex" ? { ...provider, enabled: false } : provider);
const withGrokLoggedOut = providers.map((provider) =>
	provider.id === "grok"
		? { ...provider, health: { Unhealthy: { reason: "Grok Build is not authenticated." } } }
		: provider.id === "antigravity"
			? { ...provider, health: { Degraded: { reason: "runtime not found" } } }
			: provider);

describe("model picker rows", () => {
	it("drops favorites and search hits from switched-off providers", () => {
		const favorites = [{ providerId: "codex", modelId: "gpt-6-luna", effort: "high" }];
		expect(buildModelPickerRows({ providers: withCodexOff, favorites, activeTab: FAVORITES_TAB, query: "" })).toEqual([]);
		expect(buildModelPickerRows({ providers: withCodexOff, favorites, activeTab: "claude_code", query: "gpt" })
			.filter((row) => row.providerId === "codex")).toEqual([]);
		expect(initialModelPickerTab({ providers: withCodexOff, favorites, selectedProviderId: "claude_code" }))
			.toBe("claude_code");
	});

	it("leaves providers that cannot start out of the picker", () => {
		const ids = pickerProviders(withGrokLoggedOut).map((provider) => provider.id);
		expect(ids).not.toContain("grok");
		expect(ids).not.toContain("antigravity");
		expect(ids).toContain("claude_code");
		expect(providersNeedingSetup(withGrokLoggedOut).map((provider) => provider.id))
			.toEqual(["grok", "antigravity"]);
		expect(buildModelPickerRows({ providers: withGrokLoggedOut, favorites: [], activeTab: "claude_code", query: "grok" })
			.filter((row) => row.providerId === "grok")).toEqual([]);
	});

	it("keeps the selected provider visible even when it needs setup", () => {
		expect(pickerProviders(withGrokLoggedOut, "grok").map((provider) => provider.id)).toContain("grok");
		expect(providersNeedingSetup(withGrokLoggedOut, "grok").map((provider) => provider.id))
			.toEqual(["antigravity"]);
	});

	it("searches every provider from the favorites tab", () => {
		const favorites = [{ providerId: "claude_code", modelId: "claude-haiku-5-5", effort: "high" }];
		const rows = buildModelPickerRows({ providers, favorites, activeTab: FAVORITES_TAB, query: "haiku" });
		expect(rows[0]).toMatchObject({ modelId: "claude-haiku-5-5", fromOtherProvider: false });
		expect(rows.filter((row) => row.modelId === "claude-haiku-5-5")).toHaveLength(1);
		expect(rows.some((row) => row.modelId === "claude-haiku-4-5" && row.fromOtherProvider)).toBe(true);
	});

	it("lists the open tab before matches from other providers", () => {
		const rows = buildModelPickerRows({ providers, favorites: [], activeTab: "claude_code", query: "haiku" });
		expect(rows.map((row) => [row.modelId, row.fromOtherProvider])).toEqual([
			["claude-haiku-5-5", false],
			["claude-haiku-4-5", false],
		]);
		const sonnet = buildModelPickerRows({ providers, favorites: [], activeTab: "claude_code", query: "sonnet 5.5" });
		expect(sonnet[0]).toMatchObject({ providerId: "claude_code", fromOtherProvider: false });
		expect(sonnet.some((row) => row.providerId === "droid" && row.fromOtherProvider)).toBe(true);
	});

	it("maps the platform shortcut to a row index", () => {
		const key = (init: Partial<KeyboardEvent>) => ({ key: "1", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...init });
		expect(shortcutRowIndex(key({ key: "3", metaKey: true }), true)).toBe(2);
		expect(shortcutRowIndex(key({ key: "3", ctrlKey: true }), true)).toBeNull();
		expect(shortcutRowIndex(key({ key: "1", ctrlKey: true }), false)).toBe(0);
		expect(shortcutRowIndex(key({ key: "0", metaKey: true }), true)).toBeNull();
		expect(shortcutRowIndex(key({ key: "1", metaKey: true, shiftKey: true }), true)).toBeNull();
	});

	it("adds ultrathink on top of an effort ladder only", () => {
		expect(effortSliderSteps(["low", "high"])).toEqual(["low", "high", "ultrathink"]);
		expect(effortSliderSteps([])).toEqual([]);
	});
});
