import { describe, expect, it } from "vitest";
import { resolveInitialOnboardingOpen } from "./dev-onboarding-override";

describe("resolveInitialOnboardingOpen", () => {
	it("opens when URL requests onboarding", () => {
		expect(
			resolveInitialOnboardingOpen({
				hasOnboardingQuery: true,
				isOnboardingCompleteInStorage: true,
				isDev: true,
				viteDevOnboarding: "skip",
			}),
		).toBe(true);
	});

	it("honors force in dev", () => {
		expect(
			resolveInitialOnboardingOpen({
				hasOnboardingQuery: false,
				isOnboardingCompleteInStorage: true,
				isDev: true,
				viteDevOnboarding: "force",
			}),
		).toBe(true);
	});

	it("honors skip in dev", () => {
		expect(
			resolveInitialOnboardingOpen({
				hasOnboardingQuery: false,
				isOnboardingCompleteInStorage: false,
				isDev: true,
				viteDevOnboarding: "skip",
			}),
		).toBe(false);
	});

	it("ignores override when not dev", () => {
		expect(
			resolveInitialOnboardingOpen({
				hasOnboardingQuery: false,
				isOnboardingCompleteInStorage: false,
				isDev: false,
				viteDevOnboarding: "skip",
			}),
		).toBe(true);
	});

	it("falls back to storage when dev override is unknown", () => {
		expect(
			resolveInitialOnboardingOpen({
				hasOnboardingQuery: false,
				isOnboardingCompleteInStorage: true,
				isDev: true,
				viteDevOnboarding: "maybe",
			}),
		).toBe(false);
	});
});
