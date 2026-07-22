import { describe, expect, it } from "vitest";
import { rankQuickOpenFiles } from "./file-quick-open";

const paths = [
	"src/payments/paymentBillet.ts",
	"src/payments/paymentPix.ts",
	"src/payments/payment.ts",
	"src/cache/redis-recovery.ts",
	"src/cache/redis.ts",
	"app/[token]/route.ts",
];

describe("rankQuickOpenFiles", () => {
	it("puts an exact filename before fuzzy matches", () => {
		expect(rankQuickOpenFiles(paths, "payment.ts")[0]).toBe(
			"src/payments/payment.ts",
		);
		expect(rankQuickOpenFiles(paths, "redis.ts")[0]).toBe(
			"src/cache/redis.ts",
		);
	});

	it("prioritizes direct path fragments that include route syntax", () => {
		expect(rankQuickOpenFiles(paths, "/[token]/rou")[0]).toBe(
			"app/[token]/route.ts",
		);
	});
});
