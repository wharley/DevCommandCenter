import { describe, expect, it } from "vitest";
import {
	filterFiles,
	matchFileMentionTrigger,
} from "./file-mention-plugin";

const files = [
	{ path: "src/payments/payment.ts", name: "payment.ts" },
	{ path: "src/payments/paymentBillet.ts", name: "paymentBillet.ts" },
	{ path: "src/payments/paymentPix.ts", name: "paymentPix.ts" },
	{ path: "src/cache/redis.ts", name: "redis.ts" },
	{ path: "src/cache/redis-recovery.ts", name: "redis-recovery.ts" },
	{ path: "app/[token]/route.ts", name: "route.ts" },
];

describe("file mentions", () => {
	it("keeps filename and route punctuation in the active @ query", () => {
		expect(matchFileMentionTrigger("@payment.ts")?.matchingString).toBe(
			"payment.ts",
		);
		expect(matchFileMentionTrigger("@[token]/rou")?.matchingString).toBe(
			"[token]/rou",
		);
	});

	it("returns only exact filename matches when one is typed", () => {
		expect(filterFiles(files, "payment.ts")).toEqual([files[0]]);
		expect(filterFiles(files, "redis.ts")).toEqual([files[3]]);
	});

	it("still finds a partial path containing Next.js route syntax", () => {
		expect(filterFiles(files, "[token]/rou")).toEqual([files[5]]);
	});
});
