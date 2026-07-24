import type { WorkspaceDeliveryFailureSnapshot } from "@dcc/contracts";

const AGENT_OUTPUT_MAX_CHARS = 6_000;
const AGENT_CHANGED_FILES_MAX = 50;

function boundedText(value: string, maxChars: number): string {
	if (value.length <= maxChars) return value;
	return `${value.slice(0, maxChars)}\n… [truncated for agent context]`;
}

export function buildDeliveryFailureComposerPrompt(
	failure: WorkspaceDeliveryFailureSnapshot,
): string {
	const changedFiles = failure.changedFiles.slice(0, AGENT_CHANGED_FILES_MAX);
	const parts = [
		"Investigate this DCC delivery failure using the captured context below.",
		"Verify the current workspace state before editing or retrying. If the context is stale, stop and explain what changed.",
		"Do not bypass Git hooks, force-push, or merge automatically.",
		"",
		`Operation: ${failure.operation}`,
		`Classification: ${failure.classification}`,
		`Branch: ${failure.branch ?? "unavailable"}`,
		`Commit: ${failure.headSha ?? "unavailable"}`,
		`Remote: ${failure.remote ?? "unavailable"}`,
		failure.operationTarget
			? `Operation target: ${failure.operationTarget}`
			: null,
		failure.pushTarget
			? `Push target: ${failure.pushTarget.remote}/${failure.pushTarget.branch}`
			: null,
		`Captured at: ${failure.createdAt}`,
		`Attempt token: ${failure.attemptToken}`,
		"",
		"Captured output:",
		"<delivery-failure-output>",
		boundedText(failure.output, AGENT_OUTPUT_MAX_CHARS),
		"</delivery-failure-output>",
		"",
		`Changed paths (${failure.changedFiles.length}${failure.changedFilesTruncated ? "+" : ""} captured):`,
		...(changedFiles.length > 0
			? changedFiles.map((path) => `- ${path}`)
			: ["- none"]),
		failure.changedFiles.length > changedFiles.length
			? `- … ${failure.changedFiles.length - changedFiles.length} more path(s) omitted from agent context`
			: null,
		"",
		"Explain the likely root cause, make only safe and scoped changes if needed, and tell me exactly which delivery step should be retried.",
	].filter((part): part is string => part !== null);

	return parts.join("\n");
}
