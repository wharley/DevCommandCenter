import { randomUUID } from "node:crypto";

import { resolveDccMcpToolPolicy } from "./mcp-config.mjs";
import { handlePermissionRequest } from "./permission-bridge.mjs";

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
					async (input, toolUseID, options) => {
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

						const result = await handlePermissionRequest(
							policy.toolName,
							input?.tool_input,
							{
								toolUseID,
								signal: options.signal,
								title: "DCC MCP tool approval",
								description:
									"This integration tool is configured to ask before every use.",
							},
							state,
							emit,
						);
						return result.behavior === "allow"
							? hookDecision("allow", "Approved by the user in DCC.")
							: hookDecision("deny", "Denied by the user in DCC.");
					},
				],
			},
		],
	};
}
