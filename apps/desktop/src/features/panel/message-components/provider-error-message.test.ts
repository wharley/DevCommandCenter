import { describe, expect, it } from "vitest";
import { providerErrorMessage } from "./provider-error-message";

describe("provider error message", () => {
	it("shows the exact rejection message inside an HTTP 400 envelope", () => {
		const message = "The 'gpt-6-astra' model is not supported when using Codex with a ChatGPT account.";
		expect(providerErrorMessage(JSON.stringify({
			type: "error", status: 400,
			error: { type: "invalid_request_error", message },
		}))).toBe(message);
	});

	it("unwraps an encoded provider error inside an app-server message", () => {
		const message = "Unsupported value: reasoning.effort. Supported values are low and medium.";
		expect(providerErrorMessage(JSON.stringify({ error: {
			message: JSON.stringify({ error: { message } }),
		} }))).toBe(message);
	});

	it("preserves long multiline text, including the part previously clipped", () => {
		const message = `Request rejected.\n${"More provider details. ".repeat(100)}\nContact your workspace administrator.`;
		expect(providerErrorMessage(JSON.stringify({ message }))).toBe(message);
		expect(providerErrorMessage(message)).toBe(message);
	});

	it.each([
		"Approaching rate limits",
		"You've hit your usage limit. Try again later.",
		'{"status":400,"error":{"code":"unknown"}}',
		'{"error":{"message":"The model is not',
		'{"error":{"message":""}}',
		"null",
		"[]",
		"",
	])("keeps unrecognized or plain errors faithful: %s", (reason) => {
		expect(providerErrorMessage(reason)).toBe(reason);
	});
});
