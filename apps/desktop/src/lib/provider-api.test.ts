import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: invokeMock,
}));

import { getProviderAvailability, setProviderAvailability } from "./provider-api";

describe("provider availability API", () => {
	beforeEach(() => {
		invokeMock.mockReset();
		Object.defineProperty(window, "__TAURI_INTERNALS__", {
			configurable: true,
			value: {},
		});
	});

	it("sends the typed set command and scoped payload", async () => {
		invokeMock.mockResolvedValue({
			availability: {
				providerId: "codex",
				enabled: false,
				state: "disabled",
				generation: 2,
			},
		});

		await setProviderAvailability({ providerId: "codex", enabled: false });

		expect(invokeMock).toHaveBeenCalledWith("set_provider_availability", {
			input: { providerId: "codex", enabled: false },
		});
	});

	it("sends the typed get command", async () => {
		await getProviderAvailability({ providerId: "gemini" });

		expect(invokeMock).toHaveBeenCalledWith("get_provider_availability", {
			input: { providerId: "gemini" },
		});
	});

	it("does not pretend browser fallback can persist availability", async () => {
		delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;

		await expect(
			setProviderAvailability({ providerId: "codex", enabled: true }),
		).rejects.toThrow("desktop runtime");
		expect(invokeMock).not.toHaveBeenCalled();
	});
});
