const FORCE_ONBOARDING = new Set([
	"force",
	"1",
	"true",
	"show",
	"yes",
	"on",
]);

const SKIP_ONBOARDING = new Set([
	"skip",
	"0",
	"false",
	"hide",
	"no",
	"off",
]);

/**
 * Initial onboarding visibility. In dev, `VITE_DEV_ONBOARDING` in `.env.local`
 * overrides persistence so you do not need localStorage or DB toggles.
 */
export function resolveInitialOnboardingOpen(input: {
	hasOnboardingQuery: boolean;
	isOnboardingCompleteInStorage: boolean;
	isDev: boolean;
	viteDevOnboarding?: string | null;
}): boolean {
	if (input.hasOnboardingQuery) {
		return true;
	}

	const token = input.viteDevOnboarding?.trim().toLowerCase();
	if (input.isDev && token) {
		if (FORCE_ONBOARDING.has(token)) {
			return true;
		}
		if (SKIP_ONBOARDING.has(token)) {
			return false;
		}
	}

	return !input.isOnboardingCompleteInStorage;
}
