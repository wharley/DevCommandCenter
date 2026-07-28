import { describe, expect, it } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "../../lib/fallback-provider-catalog";
import { getProviderChips } from "./provider-display";

function provider(id: string) {
	const result = FALLBACK_PROVIDER_CATALOG.providers.find((item) => item.id === id);
	if (!result) {
		throw new Error(`missing fallback provider: ${id}`);
	}
	return result;
}

describe("provider MCP support display", () => {
	it("describes native configuration without claiming a verified DCC bridge", () => {
		expect(provider("claude_code").capabilities.mcpSupport).toBe("nativeConfig");
		expect(provider("codex").capabilities.mcpSupport).toBe("nativeConfig");
		expect(getProviderChips(provider("claude_code"))).toContainEqual({
			label: "mcp native config",
			variant: "outline",
		});
	});

	it("does not show an MCP chip for providers without a supported attachment path", () => {
		for (const id of ["gemini", "droid", "cursor", "grok"]) {
			expect(provider(id).capabilities.mcpSupport).toBe("unsupported");
			expect(getProviderChips(provider(id))).not.toEqual(
				expect.arrayContaining([expect.objectContaining({ label: expect.stringContaining("mcp") })]),
			);
		}
	});

	it("reserves the verified label for a conformance-tested DCC bridge", () => {
		const verified = {
			...provider("codex"),
			capabilities: {
				...provider("codex").capabilities,
				mcpSupport: "verifiedBridge" as const,
			},
		};

		expect(getProviderChips(verified)).toContainEqual({
			label: "mcp verified",
			variant: "success",
		});
	});
});
