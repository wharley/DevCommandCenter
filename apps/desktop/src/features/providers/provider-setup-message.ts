import type { TFunction } from "i18next";

export type ProviderSetupIssue =
	| "notSignedIn"
	| "personalSignInUnsupported"
	| "runtimeMissing"
	| "invalidExecutable"
	| "notInstalled";

export type ProviderSetupMessage = {
	issue: ProviderSetupIssue;
	/** The terminal command the reason names, e.g. `grok login`. */
	command: string | null;
	/** The program that is missing, for notInstalled. */
	binary: string | null;
};

/**
 * Recognizes the setup reasons DCC's own adapters write (signed out, runtime
 * or binary missing), so the app can say them in the person's language and
 * offer setup. Anything else returns null and is shown as written.
 */
export function providerSetupMessage(reason: string): ProviderSetupMessage | null {
	const command = reason.match(/`([^`]+)`/)?.[1]?.trim() || null;
	if (/personal google sign-in is no longer supported/i.test(reason)) {
		return { issue: "personalSignInUnsupported", command: null, binary: null };
	}
	if (/configured .*executable path is invalid/i.test(reason)) {
		return { issue: "invalidExecutable", command: null, binary: null };
	}
	if (/antigravity acp runtime (?:not found|is not on path)/i.test(reason)) {
		return { issue: "runtimeMissing", command: null, binary: null };
	}
	if (/\b(?:not authenticated|not signed in|not logged in)\b/i.test(reason)) {
		return { issue: "notSignedIn", command, binary: null };
	}
	const missing = reason.match(/^failed to execute ([^\s:]+).*no such file or directory/i);
	if (missing) {
		return { issue: "notInstalled", command: null, binary: missing[1] ?? null };
	}
	return null;
}

/** The reason in the person's language when DCC knows it, else null. */
export function localizedProviderSetupReason(reason: string, t: TFunction<"common">): string | null {
	const message = providerSetupMessage(reason);
	if (!message) return null;
	switch (message.issue) {
		case "notSignedIn":
			return message.command
				? t("providerSetup.notSignedInCommand", { command: message.command })
				: t("providerSetup.notSignedIn");
		case "personalSignInUnsupported":
			return t("providerSetup.personalSignInUnsupported");
		case "runtimeMissing":
			return t("providerSetup.runtimeMissing");
		case "invalidExecutable":
			return t("providerSetup.invalidExecutable");
		case "notInstalled":
			return t("providerSetup.notInstalled", { binary: message.binary ?? "" });
	}
}
