import { describe, expect, it } from "vitest";
import type { McpIntegrationRecord } from "@dcc/contracts";
import {
	buildMcpIntegrationInput,
	createMcpIntegrationDraft,
	formatMcpTransportPreview,
	mcpIntegrationNeedsTrust,
} from "./mcp-integration-form";

describe("MCP integration form", () => {
	it("builds an HTTP integration without putting credentials in the URL", () => {
		const draft = createMcpIntegrationDraft({
			projectId: "project-1",
			sessionId: null,
		});
		draft.displayName = "Figma";
		draft.url = "https://mcp.example.test/rpc";
		draft.credentials = [
			{ id: "one", name: "Authorization", secret: "Bearer secret" },
		];

		expect(
			buildMcpIntegrationInput(draft, {
				projectId: "project-1",
				sessionId: null,
			}),
		).toEqual({
			ok: true,
			input: {
				displayName: "Figma",
				transport: {
					type: "http",
					url: "https://mcp.example.test/rpc",
				},
				scope: { type: "project", projectId: "project-1" },
				credentials: [
					{
						target: { type: "httpHeader", name: "Authorization" },
						secret: "Bearer secret",
					},
				],
			},
		});
	});

	it("keeps stdio executable and arguments structurally separate", () => {
		const draft = createMcpIntegrationDraft({
			projectId: null,
			sessionId: null,
		});
		draft.displayName = "Payments";
		draft.transport = "stdio";
		draft.executable = "/usr/local/bin/payment-mcp";
		draft.argsText = "--mode\nread only";
		draft.cwd = "/tmp/payment";
		draft.credentials = [{ id: "one", name: "PAYMENT_KEY", secret: "secret" }];

		const result = buildMcpIntegrationInput(draft, {
			projectId: null,
			sessionId: null,
		});
		expect(result).toEqual({
			ok: true,
			input: {
				displayName: "Payments",
				transport: {
					type: "stdio",
					executable: "/usr/local/bin/payment-mcp",
					args: ["--mode", "read only"],
					cwd: "/tmp/payment",
				},
				scope: { type: "global" },
				credentials: [
					{
						target: { type: "environmentVariable", name: "PAYMENT_KEY" },
						secret: "secret",
					},
				],
			},
		});
		if (result.ok) {
			expect(formatMcpTransportPreview(result.input.transport)).toBe(
				'"/usr/local/bin/payment-mcp" "--mode" "read only"',
			);
		}
	});

	it("rejects unavailable scopes, embedded URL credentials, and duplicate headers", () => {
		const draft = createMcpIntegrationDraft({
			projectId: null,
			sessionId: null,
		});
		draft.displayName = "Unsafe";
		draft.scope = "session";
		draft.url = "https://user:secret@example.test";
		expect(
			buildMcpIntegrationInput(draft, {
				projectId: null,
				sessionId: null,
			}),
		).toEqual({ ok: false, error: "url" });

		draft.url = "https://example.test";
		expect(
			buildMcpIntegrationInput(draft, {
				projectId: null,
				sessionId: null,
			}),
		).toEqual({ ok: false, error: "scope" });

		draft.scope = "global";
		draft.credentials = [
			{ id: "one", name: "Authorization", secret: "one" },
			{ id: "two", name: "authorization", secret: "two" },
		];
		expect(
			buildMcpIntegrationInput(draft, {
				projectId: null,
				sessionId: null,
			}),
		).toEqual({ ok: false, error: "duplicateCredential" });
	});

	it("requires a fresh trust decision when the fingerprint changes", () => {
		const integration = {
			definition: {
				trust: {
					currentFingerprint: "a".repeat(64),
					decision: {
						type: "trusted",
						fingerprint: "a".repeat(64),
						trustedAt: "2026-07-28T00:00:00Z",
					},
				},
			},
		} as McpIntegrationRecord;
		expect(mcpIntegrationNeedsTrust(integration)).toBe(false);

		integration.definition.trust.currentFingerprint = "b".repeat(64);
		expect(mcpIntegrationNeedsTrust(integration)).toBe(true);
	});
});
