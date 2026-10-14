import { describe, expect, it } from "vitest";
import { localizedProviderSetupReason, providerSetupMessage } from "./provider-setup-message";

const t = ((key: string, options?: Record<string, string>) =>
	options ? `${key} ${JSON.stringify(options)}` : key) as never;

describe("providerSetupMessage", () => {
	it("recognizes the sign-in reasons the adapters write, with their command", () => {
		expect(providerSetupMessage("Grok Build is not authenticated. Run `grok login` or set XAI_API_KEY."))
			.toEqual({ issue: "notSignedIn", command: "grok login", binary: null });
		expect(providerSetupMessage("Claude Code is not authenticated. Run `claude auth login`.")?.command)
			.toBe("claude auth login");
		expect(providerSetupMessage("Codex is not signed in. Run `codex login` or add a signed-in account in Settings.")?.command)
			.toBe("codex login");
		expect(providerSetupMessage("Cursor Agent is not authenticated"))
			.toEqual({ issue: "notSignedIn", command: null, binary: null });
	});

	it("recognizes a missing runtime, a missing binary and the retired Gemini login", () => {
		expect(providerSetupMessage("official Antigravity ACP runtime not found; install it or set its executable path")?.issue)
			.toBe("runtimeMissing");
		expect(providerSetupMessage("The official Antigravity ACP runtime is not on PATH; configure it.")?.issue)
			.toBe("runtimeMissing");
		expect(providerSetupMessage("failed to execute droid: No such file or directory (os error 2)"))
			.toEqual({ issue: "notInstalled", command: null, binary: "droid" });
		expect(providerSetupMessage("Personal Google sign-in is no longer supported by Gemini CLI. Configure …")?.issue)
			.toBe("personalSignInUnsupported");
	});

	it("leaves other errors as written", () => {
		expect(providerSetupMessage("rate limit exceeded")).toBeNull();
		expect(localizedProviderSetupReason("rate limit exceeded", t)).toBeNull();
		expect(localizedProviderSetupReason("Grok Build is not authenticated. Run `grok login`.", t))
			.toBe('providerSetup.notSignedInCommand {"command":"grok login"}');
	});
});
