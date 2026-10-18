import { randomUUID } from "node:crypto";

import { resolveDccMcpToolPolicy } from "./mcp-config.mjs";

function hookDecision(decision, reason) {
	return {
		hookSpecificOutput: {
			hookEventName: "PreToolUse",
			permissionDecision: decision,
			permissionDecisionReason: reason,
		},
	};
}

function emitResolvedPolicy(policy, toolUseID, emit) {
	const requestId =
		typeof toolUseID === "string" && toolUseID.trim().length > 0
			? toolUseID.trim()
			: randomUUID();
	emit({
		type: "dcc_permission_request",
		request_id: requestId,
		tool_name: policy.toolName,
		title: "DCC MCP tool policy",
		description:
			"An explicit DCC policy was applied without exposing tool arguments.",
		command: null,
		file: null,
	});
	emit({
		type: "dcc_permission_resolved",
		request_id: requestId,
		behavior: policy.decision,
	});
}

export function createDccMcpPermissionHooks(state, emit) {
	return {
		PreToolUse: [
			{
				hooks: [
					async (input, toolUseID) => {
						const policy = resolveDccMcpToolPolicy(
							state.mcpProjection,
							input?.tool_name,
						);
						if (!policy) {
							return { continue: true };
						}

						if (policy.decision === "allow") {
							emitResolvedPolicy(policy, toolUseID, emit);
							return hookDecision("allow", "Allowed by DCC MCP tool policy.");
						}
						if (policy.decision === "deny") {
							emitResolvedPolicy(policy, toolUseID, emit);
							return hookDecision("deny", "Denied by DCC MCP tool policy.");
						}

						// No explicit policy: the tool follows the task's permission
						// mode like any other tool. Full access runs it; supervised
						// modes reach canUseTool and ask through DCC.
						return { continue: true };
					},
				],
			},
		],
	};
}
