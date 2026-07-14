import { describe, expect, it } from "vitest";
import { getTerminalTabNavigationTarget } from "./terminal-tab-navigation";

describe("terminal tab keyboard navigation", () => {
	const tabs = ["one", "two", "three"];

	it("moves and wraps with arrow keys", () => {
		expect(getTerminalTabNavigationTarget(tabs, "two", "ArrowRight")).toBe("three");
		expect(getTerminalTabNavigationTarget(tabs, "three", "ArrowRight")).toBe("one");
		expect(getTerminalTabNavigationTarget(tabs, "one", "ArrowLeft")).toBe("three");
	});

	it("supports Home and End and ignores unrelated keys", () => {
		expect(getTerminalTabNavigationTarget(tabs, "two", "Home")).toBe("one");
		expect(getTerminalTabNavigationTarget(tabs, "two", "End")).toBe("three");
		expect(getTerminalTabNavigationTarget(tabs, "two", "Enter")).toBeNull();
	});
});
