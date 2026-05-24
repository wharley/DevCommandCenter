import { describe, expect, it } from "vitest";
import {
	futureOnboardingSteps,
	getNextOnboardingStep,
	getPreviousOnboardingStep,
	onboardingSteps,
} from "./OnboardingWizard.logic";

describe("OnboardingWizard.logic", () => {
	it("keeps the onboarding flow ordered", () => {
		expect(onboardingSteps).toEqual([
			"intro",
			"workflows",
			"slashCommands",
			"agents",
			"repoImport",
			"completeTransition",
		]);
	});

	it("moves forward until the last step", () => {
		expect(getNextOnboardingStep("agents")).toBe("repoImport");
		expect(getNextOnboardingStep("workflows")).toBe("slashCommands");
		expect(getNextOnboardingStep("completeTransition")).toBeNull();
	});

	it("moves backward until the first step", () => {
		expect(getPreviousOnboardingStep("repoImport")).toBe("agents");
		expect(getPreviousOnboardingStep("slashCommands")).toBe("workflows");
		expect(getPreviousOnboardingStep("intro")).toBeNull();
	});

	it("keeps future steps reserved for later phases", () => {
		expect(futureOnboardingSteps).toEqual(["corner", "skills", "conductor"]);
	});
});
