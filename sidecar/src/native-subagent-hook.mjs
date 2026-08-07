function nonEmptyString(value) {
	return typeof value === "string" && value.trim().length > 0
		? value.trim()
		: null;
}

function emitNativeSubagentActivity(input, toolUseID, status, emit) {
	const agentId = nonEmptyString(input?.agent_id);
	const agentType = nonEmptyString(input?.agent_type);
	if (!agentId || !agentType) {
		return;
	}

	// The SDK only supplies toolUseID when it can directly associate this hook
	// with a tool invocation. Keep it opaque and omit it when absent; never
	// correlate on timing, text, model, or agent type.
	const correlationId = nonEmptyString(toolUseID);
	emit({
		type: "dcc_native_subagent_activity",
		agent_id: agentId,
		agent_type: agentType,
		status,
		correlation_id: correlationId,
	});
}

export function createNativeSubagentHooks(emit) {
	return {
		SubagentStart: [
			{
				hooks: [
					async (input, toolUseID) => {
						emitNativeSubagentActivity(input, toolUseID, "running", emit);
						return { continue: true };
					},
				],
			},
		],
		SubagentStop: [
			{
				hooks: [
					async (input, toolUseID) => {
						// The hook reports a stop, not a failure reason.  Treat it as
						// completed; actual tool-result errors still become failed in
						// the Rust adapter.
						emitNativeSubagentActivity(input, toolUseID, "completed", emit);
						return { continue: true };
					},
				],
			},
		],
	};
}
