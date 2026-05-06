import { describe, expect, it } from "vitest";
import {
	clampEffort,
	highestAvailableEffort,
	isUltrathinkPrompt,
	resolveEffectiveEffort,
} from "./effort";

describe("composer effort helpers", () => {
	it("clamps legacy balanced to medium when models expose richer levels", () => {
		expect(clampEffort("balanced", ["low", "medium", "high", "xhigh"])).toBe("medium");
	});

	it("clamps unsupported higher effort down to the nearest supported level", () => {
		expect(clampEffort("max", ["low", "medium", "high"])).toBe("high");
	});

	it("finds the highest available effort for ultrathink boosts", () => {
		expect(highestAvailableEffort(["low", "medium", "high", "xhigh"])).toBe("xhigh");
	});

	it("detects ultrathink as a keyword, not just exact prompt text", () => {
		expect(isUltrathinkPrompt("please ultrathink about this change")).toBe(true);
		expect(isUltrathinkPrompt("think carefully")).toBe(false);
	});

	it("boosts to the strongest supported effort when ultrathink is selected", () => {
		expect(
			resolveEffectiveEffort({
				selectedEffort: "medium",
				supportedEfforts: ["low", "medium", "high", "max"],
				ultrathinkSelected: true,
				rawPrompt: "Investigate this failure",
			}),
		).toBe("max");
	});

	it("boosts to the strongest supported effort when the prompt contains ultrathink", () => {
		expect(
			resolveEffectiveEffort({
				selectedEffort: "medium",
				supportedEfforts: ["low", "medium", "high", "xhigh"],
				ultrathinkSelected: false,
				rawPrompt: "Ultrathink about this refactor",
			}),
		).toBe("xhigh");
	});
});
