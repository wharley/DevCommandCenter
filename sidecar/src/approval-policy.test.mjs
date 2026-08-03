import assert from "node:assert/strict";
import test from "node:test";

import { resolveClaudeApprovalOptions } from "./approval-policy.mjs";

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
