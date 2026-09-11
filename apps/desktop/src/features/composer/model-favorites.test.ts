import { beforeEach, describe, expect, it, vi } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import {
	addModelFavorite, getModelFavorites, MODEL_FAVORITES_STORAGE_KEY,
	parseModelFavorites, removeModelFavorite, resolveModelFavorite, saveModelFavorites,
} from "./model-favorites";

const high = { providerId: "codex", modelId: "gpt-6-astra", effort: "high" };
const extraHigh = { ...high, effort: "xhigh" };

beforeEach(() => saveModelFavorites([]));

describe("model favorites", () => {
	it("persists ordered combinations, allowing different efforts and providers without duplicates", () => {
		addModelFavorite(high);
		addModelFavorite(extraHigh);
		addModelFavorite(high);
		const otherProvider = { ...high, providerId: "another-provider" };
		addModelFavorite(otherProvider);
		expect(getModelFavorites()).toEqual([high, extraHigh, otherProvider]);
		saveModelFavorites([otherProvider, extraHigh, high]);
		expect(parseModelFavorites(localStorage.getItem(MODEL_FAVORITES_STORAGE_KEY)))
			.toEqual([otherProvider, extraHigh, high]);
		removeModelFavorite(extraHigh);
		expect(getModelFavorites()).toEqual([otherProvider, high]);
	});

	it("recovers valid entries from malformed storage without discarding unavailable models", () => {
		expect(parseModelFavorites("not-json")).toEqual([]);
		expect(parseModelFavorites('{}')).toEqual([]);
		expect(parseModelFavorites(JSON.stringify([null, {}, { ...high, effort: 2 }, high, high, extraHigh])))
			.toEqual([high, extraHigh]);
	});

	it("keeps favorites in memory if local storage cannot be written", () => {
		const write = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("quota"); });
		try {
			addModelFavorite(high);
			expect(getModelFavorites()).toEqual([high]);
		} finally { write.mockRestore(); }
	});

	it("resolves models only within their provider and disables unsupported combinations", () => {
		const providers = FALLBACK_PROVIDER_CATALOG.providers;
		expect(resolveModelFavorite(high, providers).available).toBe(true);
		expect(resolveModelFavorite({ ...high, effort: "ultrathink" }, providers).available).toBe(true);
		expect(resolveModelFavorite({ ...high, effort: "unknown" }, providers).available).toBe(false);
		expect(resolveModelFavorite({ ...high, providerId: "cursor" }, providers).model).toBeUndefined();
		expect(resolveModelFavorite(high, providers.map((provider) => ({ ...provider, enabled: false }))).available).toBe(false);
		const automatic = { providerId: "antigravity", modelId: "default", effort: null };
		expect(resolveModelFavorite(automatic, providers).available).toBe(true);
		expect(resolveModelFavorite({ ...automatic, effort: "high" }, providers).available).toBe(false);
	});
});
