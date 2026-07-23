import { describe, expect, it } from "vitest";
import { countActiveTerminalStatuses } from "./use-active-terminal-count";

describe("active terminal count", () => {
	it("counts only terminals that are starting or running", () => {
		expect(
			countActiveTerminalStatuses([
				"ready",
				"idle",
				"starting",
				"running",
				"exited",
				"error",
			]),
		).toBe(2);
	});
});
