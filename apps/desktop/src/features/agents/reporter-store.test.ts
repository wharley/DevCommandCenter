import { describe, expect, it } from "vitest";
import { type ReporterCleanupOffer, type ReporterRecapNews, reporterSpeech } from "./reporter-store";

const noop = () => {};
const recap: ReporterRecapNews = { text: "recap", onOpen: noop, onDismiss: noop };
const cleanup = (progress: ReporterCleanupOffer["alert"]["progress"] = null): ReporterCleanupOffer => ({
	alert: { totalBytes: 10, safeCount: 1, safeBytes: 5, progress },
	onReview: noop,
	onCleanSafe: noop,
	onDismiss: noop,
});

describe("reporterSpeech", () => {
	it("says the recap first, then the cleanup", () => {
		expect(reporterSpeech({ recap, cleanup: cleanup() })?.kind).toBe("recap");
		expect(reporterSpeech({ recap: null, cleanup: cleanup() })?.kind).toBe("cleanup");
		expect(reporterSpeech({ recap: null, cleanup: null })).toBeNull();
	});

	it("keeps talking about a cleanup under way", () => {
		expect(reporterSpeech({ recap, cleanup: cleanup({ done: 1, total: 3 }) })?.kind).toBe(
			"cleanup",
		);
	});
});
