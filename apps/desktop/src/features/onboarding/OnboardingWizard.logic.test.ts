import { describe, expect, it } from "vitest";
import {
	getNextOnboardingStep,
	getPreviousOnboardingStep,
	isLastOnboardingStep,
	onboardingSteps,
} from "./OnboardingWizard.logic";

describe("OnboardingWizard.logic", () => {
	it("keeps the onboarding flow short and ordered", () => {
		expect(onboardingSteps).toEqual(["project", "task", "workbench"]);
	});

	it("moves forward until the last step", () => {
		expect(getNextOnboardingStep("project")).toBe("task");
		expect(getNextOnboardingStep("task")).toBe("workbench");
		expect(getNextOnboardingStep("workbench")).toBeNull();
	});

	it("moves backward until the first step", () => {
		expect(getPreviousOnboardingStep("workbench")).toBe("task");
		expect(getPreviousOnboardingStep("task")).toBe("project");
		expect(getPreviousOnboardingStep("project")).toBeNull();
	});

	it("knows which step hands off to the Help panel", () => {
		expect(isLastOnboardingStep("project")).toBe(false);
		expect(isLastOnboardingStep("workbench")).toBe(true);
	});
});
