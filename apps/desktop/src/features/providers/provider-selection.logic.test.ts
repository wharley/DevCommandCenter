import { describe, expect, it } from "vitest";
import {
	resolveSelectedModelId,
	resolveSelectedProviderId,
} from "./provider-selection.logic";

function makeProvider(id: string, stable: boolean) {
	return {
		id,
		label: id.toUpperCase(),
		description: "",
		models: [
			{
				id: `${id}-default`,
				label: `${id.toUpperCase()} Default`,
				description: "",
				recommended: true,
			},
		],
		stable,
		capabilities: {
			streaming: false,
			tools: false,
			mcp: false,
			resumable: false,
			vision: false,
			experimental: false,
		},
		health: "Healthy" as const,
	};
}

describe("resolveSelectedProviderId", () => {
	it("keeps a stored provider when it still exists", () => {
		expect(
			resolveSelectedProviderId([makeProvider("alpha", false), makeProvider("beta", true)], "alpha"),
		).toBe("alpha");
	});

	it("prefers the stable provider when no stored provider is valid", () => {
		expect(
			resolveSelectedProviderId([makeProvider("alpha", false), makeProvider("beta", true)], "missing"),
		).toBe("beta");
	});

	it("falls back to the first provider when there is no stable one", () => {
		expect(
			resolveSelectedProviderId([makeProvider("alpha", false), makeProvider("beta", false)], null),
		).toBe("alpha");
	});

	it("returns null when there are no providers", () => {
		expect(resolveSelectedProviderId([], null)).toBeNull();
	});
});

describe("resolveSelectedModelId", () => {
	const provider = makeProvider("alpha", true);

	it("keeps a stored model when it still exists", () => {
		expect(resolveSelectedModelId(provider, "alpha-default")).toBe("alpha-default");
	});

	it("prefers the recommended model when the stored one is invalid", () => {
		expect(resolveSelectedModelId(provider, "missing")).toBe("alpha-default");
	});

	it("returns null when the provider has no models", () => {
		expect(resolveSelectedModelId({ ...provider, models: [] }, "missing")).toBeNull();
	});
});
