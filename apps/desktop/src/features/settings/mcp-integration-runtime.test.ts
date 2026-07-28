import { describe, expect, it } from "vitest";
import type {
	McpIntegrationRecord,
	McpRuntimeStatus,
} from "@dcc/contracts";
import {
	deriveMcpIntegrationRuntimeView,
	findOrphanMcpRuntimeStatuses,
	getMcpToolPolicyDecision,
	listMcpIntegrationTools,
	listMcpToolAnnotationHints,
	type McpRuntimeContext,
} from "./mcp-integration-runtime";

function integration(
	overrides: Partial<McpIntegrationRecord["definition"]> = {},
): McpIntegrationRecord {
	return {
		definition: {
			id: "figma",
			displayName: "Figma",
			transport: { type: "http", url: "https://mcp.example.test" },
			enabled: true,
			ownership: { type: "dccManaged" },
			trust: {
				currentFingerprint: "a".repeat(64),
				decision: {
					type: "trusted",
					fingerprint: "a".repeat(64),
					trustedAt: "2026-07-28T09:00:00Z",
				},
			},
			createdAt: "2026-07-28T09:00:00Z",
			updatedAt: "2026-07-28T09:00:00Z",
			...overrides,
		},
		bindings: [
			{
				id: "binding",
				definitionId: "figma",
				scope: { type: "project", projectId: "project-1" },
				enabled: true,
				createdAt: "2026-07-28T09:00:00Z",
				updatedAt: "2026-07-28T09:00:00Z",
			},
		],
		toolPolicies: [],
		credentialCount: 0,
	};
}

const context: McpRuntimeContext = {
	projectId: "project-1",
	sessionId: "session-1",
	sessionCreatedAt: "2026-07-28T10:00:00Z",
	providerId: "codex",
	providerSupport: "nativeConfig",
};

function status(
	overrides: Partial<McpRuntimeStatus> = {},
): McpRuntimeStatus {
	return {
		definitionId: "figma",
		providerId: "codex",
		providerVersion: "1.2.3",
		sessionId: "session-1",
		state: "connected",
		tools: [
			{
				name: "get_design",
				annotations: {
					readOnlyHint: null,
					destructiveHint: null,
					idempotentHint: null,
					openWorldHint: null,
				},
			},
		],
		checkedAt: "2026-07-28T10:01:00Z",
		boundedError: null,
		...overrides,
	};
}

describe("MCP integration runtime view", () => {
	it("uses an exact runtime snapshot even when the catalog bridge is not verified", () => {
		const view = deriveMcpIntegrationRuntimeView(
			integration(),
			[status()],
			context,
		);
		expect(view.kind).toBe("connected");
		expect(view.status?.tools).toEqual([
			{
				name: "get_design",
				annotations: {
					readOnlyHint: null,
					destructiveHint: null,
					idempotentHint: null,
					openWorldHint: null,
				},
			},
		]);
	});

	it("requires restart when configuration is newer than the runtime snapshot", () => {
		const view = deriveMcpIntegrationRuntimeView(
			integration({ updatedAt: "2026-07-28T10:02:00Z" }),
			[status()],
			context,
		);
		expect(view.kind).toBe("restartRequired");
		expect(view.status?.state).toBe("connected");
	});

	it("requires restart after disabling an integration in an older session", () => {
		const view = deriveMcpIntegrationRuntimeView(
			integration({
				enabled: false,
				updatedAt: "2026-07-28T10:02:00Z",
			}),
			[],
			context,
		);
		expect(view.kind).toBe("restartRequired");
	});

	it("requires restart when a tool policy is newer than the runtime snapshot", () => {
		const record = integration();
		record.toolPolicies = [
			{
				definitionId: "figma",
				toolName: "update_design",
				decision: "deny",
				updatedAt: "2026-07-28T10:02:00Z",
			},
		];
		expect(
			deriveMcpIntegrationRuntimeView(record, [status()], context).kind,
		).toBe("restartRequired");
	});

	it("does not infer support when a native provider has not reported status", () => {
		expect(
			deriveMcpIntegrationRuntimeView(integration(), [], context).kind,
		).toBe("notReported");
		expect(
			deriveMcpIntegrationRuntimeView(integration(), [], {
				...context,
				providerSupport: "unsupported",
			}).kind,
		).toBe("unsupported");
	});

	it("distinguishes scope and explicit provider exclusions", () => {
		const scopedElsewhere = integration();
		scopedElsewhere.bindings[0].scope = {
			type: "project",
			projectId: "project-2",
		};
		expect(
			deriveMcpIntegrationRuntimeView(scopedElsewhere, [], context).kind,
		).toBe("outOfScope");

		const excluded = integration();
		excluded.bindings[0].providerExclusions = ["codex"];
		expect(
			deriveMcpIntegrationRuntimeView(excluded, [], context).kind,
		).toBe("providerExcluded");
	});

	it("keeps removed definitions visible as orphan runtime attachments", () => {
		expect(findOrphanMcpRuntimeStatuses([], [status()])).toHaveLength(1);
		expect(findOrphanMcpRuntimeStatuses([integration()], [status()])).toEqual([]);
	});

	it("defaults unknown tools to Ask and keeps stored policies visible", () => {
		const record = integration();
		record.toolPolicies = [
			{
				definitionId: "figma",
				toolName: "legacy_mutation",
				decision: "deny",
				updatedAt: "2026-07-28T09:30:00Z",
			},
		];
		expect(
			listMcpIntegrationTools(record, [
				{ name: "read_design" },
				{ name: "legacy_mutation" },
			]),
		).toEqual(["legacy_mutation", "read_design"]);
		expect(getMcpToolPolicyDecision(record, "read_design")).toBe("ask");
		expect(getMcpToolPolicyDecision(record, "legacy_mutation")).toBe("deny");
	});

	it("shows only explicit annotation hints without changing the Ask default", () => {
		const record = integration();
		const runtimeTool = {
			name: "update_design",
			annotations: {
				readOnlyHint: false,
				destructiveHint: true,
				idempotentHint: null,
				openWorldHint: false,
			},
		};

		expect(listMcpToolAnnotationHints(runtimeTool)).toEqual([
			"mayModify",
			"destructive",
			"closedWorld",
		]);
		expect(getMcpToolPolicyDecision(record, runtimeTool.name)).toBe("ask");
		expect(
			listMcpToolAnnotationHints({
				name: "unknown",
				annotations: {
					readOnlyHint: null,
					destructiveHint: null,
					idempotentHint: null,
					openWorldHint: null,
				},
			}),
		).toEqual([]);
	});
});
