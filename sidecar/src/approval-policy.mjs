function protectedSandbox(additionalDirectories) {
	return {
		enabled: true,
		failIfUnavailable: true,
		autoAllowBashIfSandboxed: true,
		allowUnsandboxedCommands: false,
		filesystem: {
			allowWrite: [process.cwd(), ...additionalDirectories],
		},
	};
}

/**
 * Bash keeps running (git diff, grep, tests that do not write) inside an OS
 * sandbox that denies every write to the workspace roots.
 */
function readOnlySandbox(additionalDirectories) {
	return {
		enabled: true,
		failIfUnavailable: true,
		autoAllowBashIfSandboxed: true,
		allowUnsandboxedCommands: false,
		filesystem: {
			denyWrite: [process.cwd(), ...additionalDirectories],
		},
	};
}

/** File-editing tools a read-only turn never gets, whatever the prompt says. */
export const READ_ONLY_DISALLOWED_TOOLS = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

export function resolveClaudeApprovalOptions(payload, additionalDirectories) {
	if (payload?.planMode === true) {
		return {
			permissionMode: "plan",
			sandbox: protectedSandbox(additionalDirectories),
		};
	}

	switch (payload?.approvalPolicy) {
		case "read_only":
			// Delegated read-only work (review, explain). `dontAsk` denies
			// anything not pre-approved without prompting anyone, so the child
			// cannot stall on an approval and cannot write.
			return {
				permissionMode: "dontAsk",
				disallowedTools: [...READ_ONLY_DISALLOWED_TOOLS],
				sandbox: readOnlySandbox(additionalDirectories),
			};
		case "ask":
			return {
				permissionMode: "default",
				sandbox: protectedSandbox(additionalDirectories),
			};
		case "auto":
			return {
				permissionMode: "auto",
				sandbox: protectedSandbox(additionalDirectories),
			};
		case "full_access":
			return {
				permissionMode: "bypassPermissions",
				allowDangerouslySkipPermissions: true,
			};
		default:
			// Preserve the behavior of older DCC clients that do not send a policy.
			return {
				permissionMode: "acceptEdits",
				sandbox: protectedSandbox(additionalDirectories),
			};
	}
}

/**
 * Both the approval policy and the DCC MCP projection can disallow tools;
 * spreading one options object over the other would drop the first list.
 */
export function mergedDisallowedTools(...optionSets) {
	const tools = [
		...new Set(optionSets.flatMap((options) => options?.disallowedTools ?? [])),
	];
	return tools.length > 0 ? { disallowedTools: tools } : {};
}
