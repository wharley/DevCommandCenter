import { afterEach, describe, expect, it, vi } from "vitest";
import { applyDccDensity } from "./theme-provider";

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
