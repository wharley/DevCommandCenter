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

export function resolveClaudeApprovalOptions(payload, additionalDirectories) {
	if (payload?.planMode === true) {
		return {
			permissionMode: "plan",
			sandbox: protectedSandbox(additionalDirectories),
		};
	}

	switch (payload?.approvalPolicy) {
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
