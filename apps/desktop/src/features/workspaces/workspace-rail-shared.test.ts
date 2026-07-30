import { describe, expect, it } from "vitest";
import { workspaceRailStatusTakesRecapSlot } from "./workspace-rail-shared";

describe("workspace rail secondary content priority", () => {
	it("uses setup pending only when no recap exists", () => {
		expect(workspaceRailStatusTakesRecapSlot("setup_pending", false)).toBe(
			true,
		);
		expect(workspaceRailStatusTakesRecapSlot("setup_pending", true)).toBe(
			false,
		);
	});

	it("keeps initialization blocking and ready workspaces recap-first", () => {
		expect(workspaceRailStatusTakesRecapSlot("initializing", true)).toBe(true);
		expect(workspaceRailStatusTakesRecapSlot("ready", true)).toBe(false);
	});
});
