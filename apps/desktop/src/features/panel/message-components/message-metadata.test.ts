import { afterEach, describe, expect, it, vi } from "vitest";
import { formatMessageTimestamp } from "./message-metadata";

describe("formatMessageTimestamp", () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	it("uses Brazilian Portuguese when the DCC locale is pt-BR", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-07-15T18:00:00.000Z"));

		expect(
			formatMessageTimestamp(new Date("2026-07-15T16:00:00.000Z"), "pt-BR"),
		).toBe("há cerca de 2 horas");
	});

	it("uses English when the DCC locale is English", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-07-15T18:00:00.000Z"));

		expect(
			formatMessageTimestamp(new Date("2026-07-15T16:00:00.000Z"), "en"),
		).toBe("about 2 hours ago");
	});
});
