import type { SendTurnInput } from "@dcc/contracts";
import { describe, expect, it, vi } from "vitest";
import { resolveMcpTurnPreflight } from "./mcp-turn-preflight";

const input: SendTurnInput = {
	sessionId: "session-1",
	prompt: "find the newest task",
	toolInstructions: null,
	providerId: "codex",
	model: "gpt-5.6",
	providerRuntime: null,
	planMode: null,
	effort: null,
	fastMode: null,
};

describe("resolveMcpTurnPreflight", () => {
	it("opens OAuth, waits for completion, and preserves the original turn input", async () => {
		const prepareTurn = vi
			.fn()
			.mockResolvedValueOnce({
				preflight: {
					state: "authenticationRequired",
					definitionId: "clickup",
					authorizationUrl: "https://clickup.example/oauth",
				},
			})
			.mockResolvedValueOnce({ preflight: { state: "ready" } });
		const openAuthorizationUrl = vi.fn().mockResolvedValue({ ok: true });
		const waitForOauth = vi.fn().mockResolvedValue({ connected: true });

		await resolveMcpTurnPreflight(input, {
			prepareTurn,
			openAuthorizationUrl,
			waitForOauth,
		});

		expect(prepareTurn).toHaveBeenNthCalledWith(1, input);
		expect(prepareTurn).toHaveBeenNthCalledWith(2, input);
		expect(openAuthorizationUrl).toHaveBeenCalledWith(
			"https://clickup.example/oauth",
		);
		expect(waitForOauth).toHaveBeenCalledWith({
			sessionId: "session-1",
			definitionId: "clickup",
		});
	});

	it("handles independent OAuth challenges one at a time", async () => {
		const prepareTurn = vi
			.fn()
			.mockResolvedValueOnce({
				preflight: {
					state: "authenticationRequired",
					definitionId: "clickup",
					authorizationUrl: "https://clickup.example/oauth",
				},
			})
			.mockResolvedValueOnce({
				preflight: {
					state: "authenticationRequired",
					definitionId: "linear",
					authorizationUrl: "https://linear.example/oauth",
				},
			})
			.mockResolvedValueOnce({ preflight: { state: "ready" } });
		const openAuthorizationUrl = vi.fn().mockResolvedValue({ ok: true });
		const waitForOauth = vi.fn().mockResolvedValue({ connected: true });

		await resolveMcpTurnPreflight(input, {
			prepareTurn,
			openAuthorizationUrl,
			waitForOauth,
		});

		expect(openAuthorizationUrl).toHaveBeenCalledTimes(2);
		expect(waitForOauth).toHaveBeenCalledTimes(2);
	});

	it("stops instead of looping when the same integration remains challenged", async () => {
		const challenge = {
			preflight: {
				state: "authenticationRequired" as const,
				definitionId: "clickup",
				authorizationUrl: "https://clickup.example/oauth",
			},
		};
		const prepareTurn = vi.fn().mockResolvedValue(challenge);

		await expect(
			resolveMcpTurnPreflight(input, {
				prepareTurn,
				openAuthorizationUrl: vi.fn().mockResolvedValue({ ok: true }),
				waitForOauth: vi.fn().mockResolvedValue({ connected: true }),
			}),
		).rejects.toThrow(
			"MCP authentication did not become ready after authorization",
		);
	});
});
