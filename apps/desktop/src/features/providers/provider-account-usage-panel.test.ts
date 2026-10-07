import { describe, expect, it } from "vitest";
import { redeemableResets } from "./provider-account-usage-panel";

const now = Date.parse("2026-10-07T12:00:00Z");
const title = (index: number) => (index === 0 ? "next" : `Reset ${index}`);

describe("redeemableResets", () => {
	it("lists every usable credit, soonest expiry first, so each can be spent on its own", () => {
		const resets = redeemableResets({
			availableCount: 2,
			credits: [
				{ id: "late", title: "Full reset", expiresAt: "2026-10-30T10:00:00Z" },
				{ id: "gone", title: "Full reset", expiresAt: "2026-10-01T10:00:00Z" },
				{ id: "soon", title: null, expiresAt: "2026-10-22T18:08:00Z" },
			],
		}, title, now);
		expect(resets.map((reset) => [reset.id, reset.title])).toEqual([
			["soon", "Reset 1"],
			["late", "Full reset"],
		]);
	});

	it("falls back to one unnamed reset when Codex only reports a count", () => {
		expect(redeemableResets({ availableCount: 2, credits: [] }, title, now))
			.toEqual([{ id: null, title: "next", expiresAt: null }]);
		expect(redeemableResets({ availableCount: 0, credits: [] }, title, now)).toEqual([]);
	});
});
