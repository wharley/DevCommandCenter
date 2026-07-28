import type {
	McpIntegrationRecord,
	McpRuntimeState,
	McpRuntimeStatus,
	McpSupportLevel,
	McpToolSummary,
	McpToolPolicyDecision,
} from "@dcc/contracts";
import { mcpIntegrationNeedsTrust } from "./mcp-integration-form";

export type McpIntegrationRuntimeKind =
	| McpRuntimeState
	| "noSession"
	| "outOfScope"
	| "providerExcluded"
	| "notReported"
	| "restartRequired";

export type McpIntegrationRuntimeView = {
	kind: McpIntegrationRuntimeKind;
	status: McpRuntimeStatus | null;
};

export type McpRuntimeContext = {
	projectId: string | null;
	sessionId: string | null;
	sessionCreatedAt: string | null;
	providerId: string | null;
	providerSupport: McpSupportLevel | null;
};

function timestamp(value: string): number {
	const parsed = Date.parse(value);
	return Number.isFinite(parsed) ? parsed : 0;
}

function bindingMatchesScope(
	integration: McpIntegrationRecord,
	context: McpRuntimeContext,
) {
	return integration.bindings.filter((binding) => {
		if (!binding.enabled) return false;
		if (binding.scope.type === "global") return true;
		if (binding.scope.type === "project") {
			return binding.scope.projectId === context.projectId;
		}
		return binding.scope.sessionId === context.sessionId;
	});
}

export function deriveMcpIntegrationRuntimeView(
	integration: McpIntegrationRecord,
	statuses: McpRuntimeStatus[],
	context: McpRuntimeContext,
): McpIntegrationRuntimeView {
	if (!context.sessionId || !context.providerId || !context.sessionCreatedAt) {
		return { kind: "noSession", status: null };
	}

	const matchingBindings = bindingMatchesScope(integration, context);
	if (matchingBindings.length === 0) {
		return { kind: "outOfScope", status: null };
	}
	const applicableBindings = matchingBindings.filter(
		(binding) =>
			!binding.providerExclusions?.includes(context.providerId ?? ""),
	);
	if (applicableBindings.length === 0) {
		return { kind: "providerExcluded", status: null };
	}

	const status =
		statuses.find(
			(candidate) =>
				candidate.definitionId === integration.definition.id &&
				candidate.sessionId === context.sessionId &&
				candidate.providerId === context.providerId,
		) ?? null;
	const latestConfigurationAt = Math.max(
		timestamp(integration.definition.updatedAt),
		...applicableBindings.map((binding) => timestamp(binding.updatedAt)),
		...integration.toolPolicies.map((policy) => timestamp(policy.updatedAt)),
	);

	if (status) {
		if (latestConfigurationAt > timestamp(status.checkedAt)) {
			return { kind: "restartRequired", status };
		}
		return { kind: status.state, status };
	}

	if (mcpIntegrationNeedsTrust(integration)) {
		return { kind: "needsTrust", status: null };
	}
	if (!integration.definition.enabled) {
		return {
			kind:
				latestConfigurationAt > timestamp(context.sessionCreatedAt)
					? "restartRequired"
					: "disabled",
			status: null,
		};
	}
	if (latestConfigurationAt > timestamp(context.sessionCreatedAt)) {
		return { kind: "restartRequired", status: null };
	}
	if (context.providerSupport === "unsupported") {
		return { kind: "unsupported", status: null };
	}
	return { kind: "notReported", status: null };
}

export function findOrphanMcpRuntimeStatuses(
	integrations: McpIntegrationRecord[],
	statuses: McpRuntimeStatus[],
): McpRuntimeStatus[] {
	const definitionIds = new Set(
		integrations.map((integration) => integration.definition.id),
	);
	return statuses.filter((status) => !definitionIds.has(status.definitionId));
}

export function listMcpIntegrationTools(
	integration: McpIntegrationRecord,
	runtimeTools: Array<{ name: string }>,
): string[] {
	return [
		...new Set([
			...runtimeTools.map((tool) => tool.name),
			...integration.toolPolicies.map((policy) => policy.toolName),
		]),
	].sort((left, right) => left.localeCompare(right));
}

export function getMcpToolPolicyDecision(
	integration: McpIntegrationRecord,
	toolName: string,
): McpToolPolicyDecision {
	return (
		integration.toolPolicies.find((policy) => policy.toolName === toolName)
			?.decision ?? "ask"
	);
}

export type McpToolAnnotationHint =
	| "readOnly"
	| "mayModify"
	| "destructive"
	| "nonDestructive"
	| "idempotent"
	| "nonIdempotent"
	| "openWorld"
	| "closedWorld";

export function listMcpToolAnnotationHints(
	tool: McpToolSummary | undefined,
): McpToolAnnotationHint[] {
	const annotations = tool?.annotations;
	if (!annotations) {
		return [];
	}
	const hints: McpToolAnnotationHint[] = [];
	if (annotations.readOnlyHint != null) {
		hints.push(annotations.readOnlyHint ? "readOnly" : "mayModify");
	}
	if (annotations.destructiveHint != null) {
		hints.push(
			annotations.destructiveHint ? "destructive" : "nonDestructive",
		);
	}
	if (annotations.idempotentHint != null) {
		hints.push(
			annotations.idempotentHint ? "idempotent" : "nonIdempotent",
		);
	}
	if (annotations.openWorldHint != null) {
		hints.push(annotations.openWorldHint ? "openWorld" : "closedWorld");
	}
	return hints;
}
