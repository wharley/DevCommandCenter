import { describe, expect, it } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import {
	buildModelPickerRows,
	effortSliderSteps,
	FAVORITES_TAB,
	initialModelPickerTab,
	shortcutRowIndex,
} from "./model-picker.logic";

const providers = FALLBACK_PROVIDER_CATALOG.providers;
const withCodexOff = providers.map((provider) =>
	provider.id === "codex" ? { ...provider, enabled: false } : provider);

describe("model picker rows", () => {
	it("drops favorites and search hits from switched-off providers", () => {
		const favorites = [{ providerId: "codex", modelId: "gpt-6-luna", effort: "high" }];
		expect(buildModelPickerRows({ providers: withCodexOff, favorites, activeTab: FAVORITES_TAB, query: "" })).toEqual([]);
		expect(buildModelPickerRows({ providers: withCodexOff, favorites, activeTab: "claude_code", query: "gpt" })
			.filter((row) => row.providerId === "codex")).toEqual([]);
		expect(initialModelPickerTab({ providers: withCodexOff, favorites, selectedProviderId: "claude_code" }))
			.toBe("claude_code");
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
