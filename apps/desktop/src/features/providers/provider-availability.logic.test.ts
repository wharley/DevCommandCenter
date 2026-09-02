import { describe, expect, it, vi } from "vitest";
import {
	isProviderAvailabilityRequestCurrent,
	persistProviderAvailability,
} from "./provider-availability.logic";

const result = {
	availability: {
		providerId: "codex",
		enabled: false,
		state: "disabled" as const,
		generation: 4,
	},
};

describe("persistProviderAvailability", () => {
	it("persists first and then reconciles the catalog", async () => {
		const setAvailability = vi.fn().mockResolvedValue(result);
		const invalidateCatalog = vi.fn().mockResolvedValue(undefined);

		await expect(
			persistProviderAvailability(
				{ providerId: "codex", enabled: false },
				{ setAvailability, invalidateCatalog },
			),
		).resolves.toEqual(result);

		expect(setAvailability).toHaveBeenCalledWith({
			providerId: "codex",
			enabled: false,
		});
		expect(invalidateCatalog).toHaveBeenCalledOnce();
		expect(setAvailability.mock.invocationCallOrder[0]).toBeLessThan(
			invalidateCatalog.mock.invocationCallOrder[0]!,
		);
	});

	it("reconciles after persistence failure without hiding the authoritative error", async () => {
		const setAvailability = vi.fn().mockRejectedValue(new Error("backend rejected"));
		const invalidateCatalog = vi.fn().mockResolvedValue(undefined);

		await expect(
			persistProviderAvailability(
				{ providerId: "codex", enabled: true },
				{ setAvailability, invalidateCatalog },
			),
		).rejects.toThrow("backend rejected");
		expect(invalidateCatalog).toHaveBeenCalledOnce();
	});

	it("keeps the persistence error if reconciliation also fails", async () => {
		const setAvailability = vi.fn().mockRejectedValue(new Error("backend rejected"));
		const invalidateCatalog = vi.fn().mockRejectedValue(new Error("catalog unavailable"));

		await expect(
			persistProviderAvailability(
				{ providerId: "codex", enabled: true },
				{ setAvailability, invalidateCatalog },
			),
		).rejects.toThrow("backend rejected");
	});

	it("does not let a closed, remounted, or superseded request update the UI", () => {
		const token = { generation: 3, requestId: 2 };
		expect(
			isProviderAvailabilityRequestCurrent(token, {
				generation: 3,
				requestId: 2,
				mounted: true,
				open: true,
			}),
		).toBe(true);
		for (const current of [
			{ generation: 3, requestId: 1, mounted: true, open: true },
			{ generation: 4, requestId: 2, mounted: true, open: true },
			{ generation: 3, requestId: 2, mounted: true, open: false },
			{ generation: 3, requestId: 2, mounted: false, open: true },
		]) {
			expect(isProviderAvailabilityRequestCurrent(token, current)).toBe(false);
		}
	});
});
