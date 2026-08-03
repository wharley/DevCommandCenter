import assert from "node:assert/strict";
import test from "node:test";

import { normalizeDccMcpServers } from "./mcp-config.mjs";
import { createDccMcpPermissionHooks } from "./mcp-permission-hook.mjs";

function stateWith(decision) {
	return {
		mcpProjection: normalizeDccMcpServers([
			{
				definitionId: "fixture",
				name: "dcc-fixture",
				transport: { type: "http", url: "https://example.com/mcp", headers: {} },
				toolPolicies:
					decision === "ask" ? [] : [{ toolName: "mutate", decision }],
			},
		]),
		pendingPermissions: new Map(),
	};
}

function hookFor(state, events) {
	return createDccMcpPermissionHooks(state, (event) => events.push(event))
		.PreToolUse[0].hooks[0];
}

test("applies explicit DCC MCP allow and deny policies before provider permissions", async () => {
	for (const decision of ["allow", "deny"]) {
		const events = [];
		const state = stateWith(decision);
		const result = await hookFor(state, events)(
			{
				hook_event_name: "PreToolUse",
				tool_name: "mcp__dcc-fixture__mutate",
				tool_input: { secret: "not-emitted" },
			},
			`tool-${decision}`,
			{ signal: new AbortController().signal },
		);

		assert.equal(result.hookSpecificOutput.permissionDecision, decision);
		assert.deepEqual(
			events.map((event) => event.type),
			["dcc_permission_request", "dcc_permission_resolved"],
		);
		assert.equal(JSON.stringify(events).includes("not-emitted"), false);
	}
});

test("asks through DCC for MCP tools without an explicit policy", async () => {
	const events = [];
	const state = stateWith("ask");
	const pending = hookFor(state, events)(
		{
			hook_event_name: "PreToolUse",
			tool_name: "mcp__dcc-fixture__mutate",
			tool_input: {},
		},
		"tool-ask",
		{ signal: new AbortController().signal },
	);

	await Promise.resolve();
	state.pendingPermissions.get("tool-ask")?.resolve("allow");
	const result = await pending;
	assert.equal(result.hookSpecificOutput.permissionDecision, "allow");
	assert.deepEqual(
		events.map((event) => event.type),
		["dcc_permission_request", "dcc_permission_resolved"],
	);
});

test("does not intercept provider-native tools", async () => {
	const events = [];
	const state = stateWith("deny");
	const result = await hookFor(state, events)(
		{ hook_event_name: "PreToolUse", tool_name: "Bash", tool_input: {} },
		"tool-bash",
		{ signal: new AbortController().signal },
	);
	assert.deepEqual(result, { continue: true });
	assert.deepEqual(events, []);
});
