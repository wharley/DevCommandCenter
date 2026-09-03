export type OnboardingStep = "project" | "task" | "workbench";

/**
 * Three steps for the first five minutes: register a project, understand
 * that every task gets its own worktree, and learn where chat, terminal and
 * Inspector live. Depth lives in the Help panel, which the last step points to.
 */
export const onboardingSteps: readonly OnboardingStep[] = [
	"project",
	"task",
	"workbench",
] as const;

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

export function isLastOnboardingStep(step: OnboardingStep): boolean {
	return getNextOnboardingStep(step) === null;
}
