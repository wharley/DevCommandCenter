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
	"- When a phase of the plan, or independent multi-step work, deserves its own conversation with a clean context, call dcc_propose_task when it is in your tool list: a short title in the human's language and a self-contained brief. It runs on your own provider unless you name another.",
	"- The human starts it from a card; nothing runs before that. Keep quick work here (a question, a read, one edit) and mention small follow-ups in your reply instead of proposing them. At most 3 proposals per turn.",
	"- When your reply names real multi-step work for later (the next phase of the plan, a separate piece, a follow-up PR), propose it with dcc_propose_task before you end the turn instead of only describing it: the card is how the human says yes. A one-line tweak stays a sentence in your reply.",
	"- Each time a task ends a turn, DCC sends you a message starting with [DCC] Task. Tell the human where the plan stands and propose the next step if one is due; do not start work on your own from that message.",
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
		"- To hand a bounded, self-contained task to a different provider, call the DCC tool dcc_delegate_task when it is in your tool list.",
		"- It returns a taskId at once; DCC delivers the result later as a message starting with [DCC] (mid-turn if you are still working). Keep working on independent parts or end your turn — do not poll. Use wait=true or dcc_task_wait only when you cannot continue without the result.",
		"- review and explain are read-only; implement edits an isolated worktree that the human reviews before anything reaches this workspace.",
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
