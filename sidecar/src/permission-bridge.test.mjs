import assert from "node:assert/strict";
import test from "node:test";

import { handlePermissionRequest } from "./permission-bridge.mjs";

function permissionOptions(toolUseID) {
	return {
		toolUseID,
		title: "Fixture mutation",
		description: "Must cross the DCC approval boundary",
		signal: new AbortController().signal,
	};
}

async function waitForPending(state, requestId) {
	for (let attempt = 0; attempt < 20; attempt += 1) {
		const pending = state.pendingPermissions.get(requestId);
		if (pending) {
			return pending;
		}
		await new Promise((resolve) => setImmediate(resolve));
	}
	throw new Error("permission request was not registered");
}

test("an MCP mutation cannot execute before an explicit DCC response", async () => {
	const state = { pendingPermissions: new Map() };
	const events = [];
	const request = handlePermissionRequest(
		"mcp__dcc-fixture__fixture_mutate",
		{ label: "must-not-run", changeTools: true },
		permissionOptions("permission-1"),
		state,
		(event) => events.push(event),
	);

	const pending = await waitForPending(state, "permission-1");
	assert.equal(events.length, 1);
	assert.deepEqual(events[0], {
		type: "dcc_permission_request",
		request_id: "permission-1",
		tool_name: "mcp__dcc-fixture__fixture_mutate",
		title: "Fixture mutation",
		description: "Must cross the DCC approval boundary",
		command: null,
		file: null,
	});

	pending.resolve("deny");
	assert.deepEqual(await request, {
		behavior: "deny",
		message: "User denied tool execution.",
	});
	assert.equal(state.pendingPermissions.size, 0);
	assert.deepEqual(events[1], {
		type: "dcc_permission_resolved",
		request_id: "permission-1",
		behavior: "deny",
	});
});

test("an explicit allow preserves the exact provider input", async () => {
	const state = { pendingPermissions: new Map() };
	const events = [];
	const input = { value: "dcc-conformance-echo-v1" };
	const request = handlePermissionRequest(
		"mcp__dcc-fixture__fixture_echo",
		input,
		permissionOptions("permission-2"),
		state,
		(event) => events.push(event),
	);

	const pending = await waitForPending(state, "permission-2");
	pending.resolve("allow");

	assert.deepEqual(await request, {
		behavior: "allow",
		updatedInput: input,
	});
	assert.equal(events[1].behavior, "allow");
});

test("aborting a pending MCP permission fails closed", async () => {
	const state = { pendingPermissions: new Map() };
	const events = [];
	const controller = new AbortController();
	const request = handlePermissionRequest(
		"mcp__dcc-fixture__fixture_mutate",
		{},
		{
			...permissionOptions("permission-3"),
			signal: controller.signal,
		},
		state,
		(event) => events.push(event),
	);

	await waitForPending(state, "permission-3");
	controller.abort();

	assert.equal((await request).behavior, "deny");
	assert.equal(events[1].behavior, "deny");
	assert.equal(state.pendingPermissions.size, 0);
});
