import { afterEach, describe, expect, it, vi } from "vitest";
import {
	applyDccDensity,
	isDccThemePreference,
	resolveDccTheme,
} from "./theme-provider";

describe("applyDccDensity", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("applies the density preference to the document root", () => {
		const dataset: Record<string, string> = {};
		vi.stubGlobal("document", { documentElement: { dataset } });

		applyDccDensity("compact");
		expect(dataset.density).toBe("compact");

		applyDccDensity("comfortable");
		expect(dataset.density).toBe("comfortable");
	});
});

describe("theme preference", () => {
	it("follows the OS only when the preference is system", () => {
		expect(resolveDccTheme("system", "dark")).toBe("dark");
		expect(resolveDccTheme("system", "light")).toBe("light");
		expect(resolveDccTheme("light", "dark")).toBe("light");
		expect(resolveDccTheme("dark", "light")).toBe("dark");
	});

	it("accepts only known stored values", () => {
		expect(isDccThemePreference("system")).toBe(true);
		expect(isDccThemePreference("light")).toBe(true);
		expect(isDccThemePreference("dark")).toBe(true);
		expect(isDccThemePreference("auto")).toBe(false);
		expect(isDccThemePreference(null)).toBe(false);
	});
});
