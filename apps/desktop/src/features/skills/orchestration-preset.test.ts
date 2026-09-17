import { describe, expect, it } from "vitest";
import { ORCHESTRATION_PRESET } from "./orchestration-preset";

describe("orchestration preset", () => {
	it("offers the preset to every supported selectable provider", () => {
		expect(ORCHESTRATION_PRESET.name).toBe("dcc-orchestration");
		expect(ORCHESTRATION_PRESET.targetAgents).toEqual([
			"claude",
			"codex",
			"gemini",
			"cursor",
			"grok",
		]);
		expect(ORCHESTRATION_PRESET.disableModelInvocation).toBe(false);
		expect(ORCHESTRATION_PRESET.scope).toBe("project");
	});
});
