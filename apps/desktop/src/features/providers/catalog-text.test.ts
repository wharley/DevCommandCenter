import { describe, expect, it } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import { modelDescriptionText, providerDescriptionText } from "./catalog-text";

const t = ((key: string) => `t:${key}`) as never;
const grok = FALLBACK_PROVIDER_CATALOG.providers.find((provider) => provider.id === "grok")!;

describe("catalog text", () => {
	it("says known providers and model descriptions through i18n", () => {
		expect(providerDescriptionText(grok, t)).toBe("t:catalogText.provider.grok");
		expect(modelDescriptionText("Model available to the authenticated Antigravity account.", t))
			.toBe("t:catalogText.model.antigravityAccount");
	});

	it("shows anything unknown as the backend wrote it", () => {
		expect(providerDescriptionText({ ...grok, id: "new_agent", description: "New agent." }, t))
			.toBe("New agent.");
		expect(modelDescriptionText("A brand new model.", t)).toBe("A brand new model.");
	});
});
