import assert from "node:assert/strict";
import test from "node:test";

import { createNativeSubagentHooks } from "./native-subagent-hook.mjs";

function hookFor(event, events) {
	return createNativeSubagentHooks((item) => events.push(item))[event][0].hooks[0];
}

test("emits structured Claude native subagent lifecycle and preserves a real tool id", async () => {
	const events = [];
	await hookFor("SubagentStart", events)(
		{ hook_event_name: "SubagentStart", agent_id: "agent-1", agent_type: "Explore" },
		"toolu_agent",
	);
	await hookFor("SubagentStop", events)(
		{ hook_event_name: "SubagentStop", agent_id: "agent-1", agent_type: "Explore" },
		"toolu_agent",
	);
	assert.deepEqual(events, [
		{
			type: "dcc_native_subagent_activity",
			agent_id: "agent-1",
			agent_type: "Explore",
			status: "running",
			correlation_id: "toolu_agent",
		},
		{
			type: "dcc_native_subagent_activity",
			agent_id: "agent-1",
			agent_type: "Explore",
			status: "completed",
			correlation_id: "toolu_agent",
		},
	]);
});

test("does not invent correlation or emit incomplete hook payloads", async () => {
	const events = [];
	await hookFor("SubagentStart", events)(
		{ hook_event_name: "SubagentStart", agent_id: "agent-1", agent_type: "Explore" },
		undefined,
	);
	await hookFor("SubagentStart", events)(
		{ hook_event_name: "SubagentStart", agent_id: "", agent_type: "Explore" },
		"toolu_agent",
	);
	assert.deepEqual(events, [
		{
			type: "dcc_native_subagent_activity",
			agent_id: "agent-1",
			agent_type: "Explore",
			status: "running",
			correlation_id: null,
		},
	]);
});
