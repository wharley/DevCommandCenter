import { describe, expect, it } from "vitest";
import type { ProviderCatalog } from "@dcc/contracts";
import {
	presentableNativeSubagentName,
	resolveNativeSubagentModelName,
	resolveNativeSubagentPresentation,
} from "./native-subagent-presentation";

describe("native subagent presentation", () => {
	it("uses the complete model id when no catalog label is available", () => {
		expect(resolveNativeSubagentModelName("gpt-5.6-terra")).toBe(
			"gpt-5.6-terra",
		);
		expect(resolveNativeSubagentModelName("gpt-5.6-luna")).toBe(
			"gpt-5.6-luna",
		);
	});

	it("keeps model names without a Codex codename intact", () => {
		expect(resolveNativeSubagentModelName("gpt-5.5")).toBe("gpt-5.5");
		expect(resolveNativeSubagentModelName("claude-opus-5")).toBe("claude-opus-5");
	});

	it("uses the same complete catalog label shown for the principal agent", () => {
		const providers = [
			{
				models: [{ id: "gpt-5.6-luna", label: "GPT-5.6 Luna" }],
			},
		] as unknown as ProviderCatalog["providers"];
		expect(resolveNativeSubagentModelName("gpt-5.6-luna", providers)).toBe(
			"GPT-5.6 Luna",
		);
	});

	it("does not expose a canonical task path as the agent name", () => {
		expect(presentableNativeSubagentName("/root/atualizar_hero")).toBeNull();
		expect(presentableNativeSubagentName("root/reviewer")).toBeNull();
		expect(presentableNativeSubagentName("Lorentz")).toBe("Lorentz");
	});

	it("renders the requested spawn model instead of the task path", () => {
		expect(
			resolveNativeSubagentPresentation({
				model: null,
				requestedModel: "gpt-5.6-terra",
				name: "/root/atualizar_hero",
				role: null,
			}),
		).toMatchObject({ identity: "gpt-5.6-terra", agentName: null });
	});
});
