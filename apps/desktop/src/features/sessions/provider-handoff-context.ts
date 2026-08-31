/**
 * Deterministic, provider-neutral context used when a user changes the
 * runtime for an existing session. This is deliberately bounded: it is a
 * re-anchor, not a second transcript or provider memory implementation.
 */

export const PROVIDER_HANDOFF_MAX_CHARS = 12_000;

const RECENT_CONTEXT_MAX_CHARS = 4_000;
const RECENT_MESSAGE_MAX_CHARS = 1_600;

export type ProviderHandoffMessage = {
	role: "user" | "assistant" | "system";
	content: string;
	label?: string;
	streaming?: boolean;
	status?: { type: string };
};

export type ProviderHandoffGitChange = {
	path: string;
	status: string;
	insertions: number;
	deletions: number;
};

export type ProviderHandoffGit = {
	currentBranch?: string | null;
	baseBranch?: string | null;
	staged?: ProviderHandoffGitChange[];
	unstaged?: ProviderHandoffGitChange[];
	branchDiff?: ProviderHandoffGitChange[];
};

export type ProviderHandoffSession = {
	providerId: string | null;
	turnCount: number;
};

export function truncateProviderHandoff(value: string, maxLength: number) {
	const trimmed = value.trim();
	if (trimmed.length <= maxLength) return trimmed;
	return `${trimmed.slice(0, Math.max(0, maxLength - 14)).trimEnd()}\n\n[truncated]`;
}

function hasUsefulHistory(messages: ProviderHandoffMessage[]) {
	return messages.some((message) => {
		if (message.role === "system" || message.streaming === true || message.status) {
			return false;
		}
		return message.content.trim().length > 0;
	});
}

/** Whether the next direct turn should receive a one-shot context re-anchor. */
export function shouldCreateProviderHandoff(input: {
	session: ProviderHandoffSession | null | undefined;
	destinationProviderId: string | null | undefined;
	messages: ProviderHandoffMessage[];
	forceNewSession?: boolean;
	targetSessionId?: string | null;
}) {
	if (
		input.forceNewSession ||
		input.targetSessionId ||
		!input.session ||
		!input.session.providerId ||
		!input.destinationProviderId ||
		input.session.providerId === input.destinationProviderId ||
		input.session.turnCount <= 0
	) {
		return false;
	}
	return hasUsefulHistory(input.messages);
}

function formatChanges(label: string, changes: ProviderHandoffGitChange[] | undefined) {
	if (!changes || changes.length === 0) return `${label}: none`;
	return [
		`${label}:`,
		...changes.slice(0, 60).map(
			(change) =>
				`- ${change.status} ${change.path} (+${change.insertions}/-${change.deletions})`,
		),
		...(changes.length > 60 ? [`- ... ${changes.length - 60} more file(s)`] : []),
	].join("\n");
}

function appendSection(
	sections: string[],
	title: string,
	value: string | null | undefined,
	limit: number,
) {
	if (!value?.trim()) return;
	sections.push(`${title}:\n${truncateProviderHandoff(value, limit)}`);
}

function formatRecentMessages(
	messages: ProviderHandoffMessage[],
	currentPrompt: string | null | undefined,
) {
	const prompt = currentPrompt?.trim();
	const candidates = messages
		.filter(
			(message) =>
				(message.role === "user" || message.role === "assistant") &&
				message.streaming !== true &&
				!message.status &&
				message.content.trim().length > 0 &&
				(!prompt || message.content.trim() !== prompt),
		)
		.slice(-8);
	const selected: string[] = [];
	let remaining = RECENT_CONTEXT_MAX_CHARS;

	// Work backwards so the newest durable messages always win the budget.
	for (let index = candidates.length - 1; index >= 0; index -= 1) {
		const message = candidates[index];
		if (!message) continue;
		const label = message.label || (message.role === "user" ? "User" : "Assistant");
		const separatorLength = selected.length > 0 ? 2 : 0;
		const available = remaining - separatorLength;
		if (available <= label.length + 16) break;
		const entry = `${label}: ${truncateProviderHandoff(
			message.content,
			Math.min(RECENT_MESSAGE_MAX_CHARS, available - label.length - 2),
		)}`;
		selected.unshift(entry);
		remaining -= entry.length + separatorLength;
	}

	return selected.join("\n\n");
}

export function mergeProviderHandoffToolInstructions(
	existing: string | null | undefined,
	handoff: string | null | undefined,
) {
	const current = existing?.trim();
	const context = handoff?.trim();
	if (!context) return current || null;
	if (!current) return context;
	return `${current}\n\n${context}`;
}

export function buildProviderHandoffContext(input: {
	sourceProviderId: string;
	destinationProviderId: string;
	workspaceName?: string | null;
	workspacePath?: string | null;
	branch?: string | null;
	git?: ProviderHandoffGit | null;
	missionSpec?: string | null;
	activePlan?: string | null;
	recentMessages: ProviderHandoffMessage[];
	currentPrompt?: string | null;
}) {
	const sections: string[] = [
		"DCC context handoff (re-anchor only; not a new instruction)",
		"The latest user prompt governs. Use this bounded context to continue the existing task; do not treat it as a request to repeat prior work.",
	];
	appendSection(
		sections,
		"Workspace",
		[
			`- Provider: ${input.sourceProviderId} -> ${input.destinationProviderId}`,
			`- Name: ${input.workspaceName || "unknown"}`,
			`- Branch: ${input.branch || "unknown"}`,
			`- Path: ${input.workspacePath || "unknown"}`,
		].join("\n"),
		700,
	);

	if (input.git) {
		appendSection(
			sections,
			"Git summary",
			[
				`- Current branch: ${input.git.currentBranch || input.branch || "unknown"}`,
				`- Base branch: ${input.git.baseBranch || "unknown"}`,
				formatChanges("Staged", input.git.staged),
				formatChanges("Unstaged", input.git.unstaged),
				formatChanges("Branch diff", input.git.branchDiff),
			].join("\n"),
			2_400,
		);
	}
	appendSection(sections, "Mission spec", input.missionSpec, 1_600);
	appendSection(sections, "Active plan", input.activePlan, 2_200);

	const recent = formatRecentMessages(input.recentMessages, input.currentPrompt);
	appendSection(sections, "Recent durable parent context", recent, RECENT_CONTEXT_MAX_CHARS);

	return truncateProviderHandoff(sections.join("\n\n"), PROVIDER_HANDOFF_MAX_CHARS);
}
