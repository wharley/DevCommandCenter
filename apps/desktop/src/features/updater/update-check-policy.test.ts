import { describe, expect, it } from "vitest";
import {
	automaticUpdateCheckIsDue,
	UPDATE_CHECK_INTERVAL_MS,
} from "./update-check-policy";

describe("automatic update checks", () => {
	it("does not repeat a focus check during the cooldown", () => {
		expect(automaticUpdateCheckIsDue(1_000, 1_000 + 4 * 60 * 1_000)).toBe(false);
	});

	it("checks again after five active minutes", () => {
		expect(automaticUpdateCheckIsDue(1_000, 1_000 + 5 * 60 * 1_000)).toBe(true);
	});

	it("keeps the periodic refresh intentionally infrequent", () => {
		expect(UPDATE_CHECK_INTERVAL_MS).toBe(30 * 60 * 1_000);
	});
});
