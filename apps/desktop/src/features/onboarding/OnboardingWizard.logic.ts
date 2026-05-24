export type OnboardingStep =
	| "intro"
	| "workflows"
	| "slashCommands"
	| "agents"
	| "repoImport"
	| "completeTransition";

export const onboardingSteps: readonly OnboardingStep[] = [
	"intro",
	"workflows",
	"slashCommands",
	"agents",
	"repoImport",
	"completeTransition",
] as const;

export const futureOnboardingSteps = ["corner", "skills", "conductor"] as const;

export function getNextOnboardingStep(step: OnboardingStep): OnboardingStep | null {
	const index = onboardingSteps.indexOf(step);
	return index >= 0 && index < onboardingSteps.length - 1
		? onboardingSteps[index + 1]!
		: null;
}

export function getPreviousOnboardingStep(step: OnboardingStep): OnboardingStep | null {
	const index = onboardingSteps.indexOf(step);
	return index > 0 ? onboardingSteps[index - 1]! : null;
}
