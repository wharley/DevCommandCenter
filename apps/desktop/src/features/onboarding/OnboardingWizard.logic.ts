export type OnboardingStep =
	| "intro"
	| "agents"
	| "repoImport"
	| "completeTransition";

export const onboardingSteps: readonly OnboardingStep[] = [
	"intro",
	"agents",
	"repoImport",
	"completeTransition",
] as const;

export const futureOnboardingSteps = ["corner", "skills", "conductor"] as const;

const stepMeta: Record<OnboardingStep, { title: string; body: string }> = {
	intro: {
		title: "Welcome to Dev Command Center",
		body: "Start from the shell, keep your workspace visible, and let the thread stay in the center.",
	},
	agents: {
		title: "Agents stay in the loop",
		body: "The inspector and composer are already primed for provider-driven sessions and runtime context.",
	},
	repoImport: {
		title: "Import a repository",
		body: "Point the shell at a local repo or clone from URL and the workspace opens around it.",
	},
	completeTransition: {
		title: "Ready to transition",
		body: "Finish onboarding and drop into the shell with the same chrome the blueprint calls for.",
	},
};

export function getOnboardingStepMeta(step: OnboardingStep) {
	return stepMeta[step];
}

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
