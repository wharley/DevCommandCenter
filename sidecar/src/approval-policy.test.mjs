import assert from "node:assert/strict";
import test from "node:test";

import {
	mergedDisallowedTools,
	READ_ONLY_DISALLOWED_TOOLS,
	resolveClaudeApprovalOptions,
} from "./approval-policy.mjs";

test("plan mode always overrides the selected approval policy", () => {
	const options = resolveClaudeApprovalOptions(
		{ planMode: true, approvalPolicy: "full_access" },
		["/shared"],
	);

	assert.equal(options.permissionMode, "plan");
	assert.equal(options.allowDangerouslySkipPermissions, undefined);
	assert.equal(options.sandbox.enabled, true);
});

test("full access uses Claude's explicit native bypass flag", () => {
	assert.deepEqual(
		resolveClaudeApprovalOptions(
			{ planMode: false, approvalPolicy: "full_access" },
			[],
		),
		{
			permissionMode: "bypassPermissions",
			allowDangerouslySkipPermissions: true,
		},
	);
});

test("automatic approval keeps the task sandbox", () => {
	const options = resolveClaudeApprovalOptions(
		{ planMode: false, approvalPolicy: "auto" },
		["/shared"],
	);

	assert.equal(options.permissionMode, "auto");
	assert.deepEqual(options.sandbox.filesystem.allowWrite, [
		process.cwd(),
		"/shared",
	]);
});

test("unknown policies fail closed to the legacy protected behavior", () => {
	const options = resolveClaudeApprovalOptions(
		{ planMode: false, approvalPolicy: "unexpected" },
		[],
	);

	assert.equal(options.permissionMode, "acceptEdits");
	assert.equal(options.sandbox.enabled, true);
});

test("read-only turns cannot edit files or write from Bash, and never prompt", () => {
	const options = resolveClaudeApprovalOptions(
		{ planMode: false, approvalPolicy: "read_only" },
		["/shared"],
	);

	assert.equal(options.permissionMode, "dontAsk");
	assert.deepEqual(options.disallowedTools, READ_ONLY_DISALLOWED_TOOLS);
	assert.equal(options.sandbox.enabled, true);
	assert.equal(options.sandbox.failIfUnavailable, true);
	assert.equal(options.sandbox.allowUnsandboxedCommands, false);
	assert.deepEqual(options.sandbox.filesystem.denyWrite, [process.cwd(), "/shared"]);
	assert.equal(options.sandbox.filesystem.allowWrite, undefined);
	assert.equal(options.allowDangerouslySkipPermissions, undefined);
});

test("disallowed tools from the policy and the MCP projection are merged", () => {
	assert.deepEqual(
		mergedDisallowedTools(
			{ disallowedTools: ["Edit", "Write"] },
			{ disallowedTools: ["mcp__dcc__x", "Edit"] },
		),
		{ disallowedTools: ["Edit", "Write", "mcp__dcc__x"] },
	);
	assert.deepEqual(mergedDisallowedTools({}, {}), {});
});
