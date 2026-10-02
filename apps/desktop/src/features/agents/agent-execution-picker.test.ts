import { describe, expect, it } from "vitest";
import { agentEffortForModel } from "./agent-execution-picker";

describe("agentEffortForModel", () => {
	it("keeps a level the model supports and clamps one it does not", () => {
		expect(agentEffortForModel("high", ["low", "medium", "high"])).toBe("high");
		expect(agentEffortForModel("xhigh", ["low", "medium", "high"])).toBe("high");
	});

	it("is null when the agent has no effort or the model takes none", () => {
		expect(agentEffortForModel(null, ["low", "high"])).toBeNull();
		expect(agentEffortForModel("high", [])).toBeNull();
		expect(agentEffortForModel("high", undefined)).toBeNull();
	});
});
