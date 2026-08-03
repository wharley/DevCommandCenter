import { describe, expect, it } from "vitest";
import { countActiveTerminalStatuses } from "./use-active-terminal-count";

describe("active terminal count", () => {
	it("counts only commands that are starting, running, or waiting", () => {
		expect(
			countActiveTerminalStatuses([
				"ready",
				"idle",
				"starting",
				"running",
				"waiting",
				"exited",
				"error",
			]),
		).toBe(3);
	});
});
