import type { ProviderCatalog } from "@dcc/contracts";

type Provider = ProviderCatalog["providers"][number];

function buildNativeSubagentInstructions(provider: Provider) {
	const modelLabels = provider.models.map((model) => model.label).join(", ");
	const shared = [
		"Delegation routing rule:",
		"- dcc_delegate_task is exclusively for delegation to a different provider.",
		`- Models belonging to the active provider (${provider.label}) are native subagent requests, not dcc_delegate_task targets.`,
		modelLabels ? `- Active-provider models: ${modelLabels}.` : null,
	];

	if (provider.id === "claude_code") {
		return [
			...shared,
			'- When the user requests a Claude subagent or a Claude model family, use Claude\'s native Agent tool. Set Agent.model to the matching family alias: "opus", "sonnet", or "haiku". For example, "Opus 5.5" means Agent.model = "opus".',
			"- Do not search for a Claude model in the external-provider target list and do not ask the user to choose another provider for a Claude-native request.",
		];
	}

	if (provider.id === "codex") {
		return [
			...shared,
			"- When the user requests a Codex subagent or Codex model, use Codex's native spawn_agent tool and pass the requested model ID in its model field.",
			"- Do not search for a Codex model in the external-provider target list and do not ask the user to choose another provider for a Codex-native request.",
		];
	}

	return [
		...shared,
		"- When the active provider offers a native subagent mechanism, use it for an active-provider model request.",
	];
}

/** `dcc_propose_task`: a phase of the plan as its own conversation, on any provider. */
const TASK_PROPOSAL_INSTRUCTIONS = [
	"Task proposals (Dev Command Center):",
	"- What the human asks for in this conversation is yours to do here: investigate, implement, review, fix and test it yourself. Never split their request into tasks.",
	"- Call dcc_propose_task, when it is in your tool list, only when the human asks to split work off, run it in parallel or open another task, or for an independent piece of work outside their request that you noticed. It takes a short title in the human's language and a self-contained brief, and runs on your own provider unless you name another.",
	"- The human starts it from a card; nothing runs before that. At most 3 proposals per turn. A small follow-up or a one-line tweak stays a sentence in your reply.",
	"- Never propose a task to review, re-review, validate or fix another task's work. Its edits are already in your checkout: read the diff, fix what is wrong and run the checks yourself, here.",
	"- A task that edits works in your checkout, like a subagent: its edits are already there when it reports back, with no review step. Only one task edits at a time: DCC refuses to start another while one is working.",
	"- Each time a task ends a turn, DCC sends you a message starting with [DCC] Task. Tell the human where things stand in a few lines and stop: that message cannot propose or start tasks.",
	"- To send an open task a follow-up, call dcc_task_message from a turn the human started; it runs as a turn of its own in that task and reports back the same way.",
];

export function buildDelegateTaskToolInstructions(
	providers: ProviderCatalog["providers"],
	currentProviderId: string,
) {
	const currentProvider = providers.find(
		(provider) => provider.id === currentProviderId,
	);
	if (!currentProvider) {
		return "";
	}

	const lines = [
		...buildNativeSubagentInstructions(currentProvider),
		"",
		...TASK_PROPOSAL_INSTRUCTIONS,
	];
	const targets = providers.filter(
		(provider) =>
			provider.id !== currentProviderId &&
			provider.capabilities.canBeDelegationTarget &&
			provider.capabilities.supportsReadOnlyDelegation,
	);
	if (targets.length === 0) {
		return lines.filter(Boolean).join("\n");
	}

	return [
		...lines,
		"",
		"Cross-provider delegation (Dev Command Center):",
		"- When the human asks for another provider or model to do a bounded, self-contained piece, call the DCC tool dcc_delegate_task when it is in your tool list. Do not delegate their request on your own.",
		"- It returns a taskId at once; DCC delivers the result later as a message starting with [DCC] (mid-turn if you are still working). Keep working on independent parts or end your turn — do not poll. Use wait=true or dcc_task_wait only when you cannot continue without the result.",
		"- review and explain are read-only; implement edits this checkout directly, so its edits are already here when the result arrives.",
		"- Pass a short title (a few words, in the human's language) naming the task; the human sees it as the child conversation's title.",
		`Available external-provider delegation targets: ${targets
			.map((provider) => `${provider.id} (${provider.label})`)
			.join(", ")}.`,
	].join("\n");
}

export function resolveDelegateTaskToolInstructions({
	provider,
	providers,
}: {
	provider: Provider | null | undefined;
	providers: ProviderCatalog["providers"];
}) {
	if (!provider?.capabilities.canRequestDelegation) {
		return null;
	}
	const instructions = buildDelegateTaskToolInstructions(providers, provider.id);
	return instructions || null;
}
