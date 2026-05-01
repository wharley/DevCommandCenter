import { describe, expect, it } from "vitest";
import {
	futureOnboardingSteps,
	getNextOnboardingStep,
	getOnboardingStepMeta,
	getPreviousOnboardingStep,
	onboardingSteps,
} from "./OnboardingWizard.logic";

describe("OnboardingWizard.logic", () => {
	it("keeps the onboarding flow ordered", () => {
		expect(onboardingSteps).toEqual([
			"intro",
			"agents",
			"repoImport",
			"completeTransition",
		]);
	});

	it("describes the intro step", () => {
		expect(getOnboardingStepMeta("intro")).toEqual({
			title: "Welcome to Dev Command Center",
			body: "Start from the shell, keep your workspace visible, and let the thread stay in the center.",
		});
	});

	it("moves forward until the last step", () => {
		expect(getNextOnboardingStep("agents")).toBe("repoImport");
		expect(getNextOnboardingStep("completeTransition")).toBeNull();
	});

	it("moves backward until the first step", () => {
		expect(getPreviousOnboardingStep("repoImport")).toBe("agents");
		expect(getPreviousOnboardingStep("intro")).toBeNull();
	});

	it("keeps future steps reserved for later phases", () => {
		expect(futureOnboardingSteps).toEqual(["corner", "skills", "conductor"]);
	});
});
