import { describe, expect, it } from "vitest";
import { ORCHESTRATION_PRESET } from "./orchestration-preset";

describe("orchestration preset", () => {
	it("is an implicitly invokable Codex-native project skill", () => {
		expect(ORCHESTRATION_PRESET.name).toBe("dcc-orchestration");
		expect(ORCHESTRATION_PRESET.targetAgents).toEqual(["codex"]);
		expect(ORCHESTRATION_PRESET.disableModelInvocation).toBe(false);
		expect(ORCHESTRATION_PRESET.description).toContain("Codex subagents");
		expect(ORCHESTRATION_PRESET.body).toContain("gpt-5.6-terra");
		expect(ORCHESTRATION_PRESET.body).toContain("Wait for every required result");
	});
});
