import type { ProviderCatalog } from "@dcc/contracts";

type Provider = ProviderCatalog["providers"][number];

function buildNativeSubagentInstructions(provider: Provider) {
	const modelLabels = provider.models.map((model) => model.label).join(", ");
	const shared = [
		"Delegation routing rule:",
		"- delegate_task is exclusively for delegation to a different provider.",
		`- Models belonging to the active provider (${provider.label}) are native subagent requests, not delegate_task targets.`,
		modelLabels ? `- Active-provider models: ${modelLabels}.` : null,
	];

	if (provider.id === "claude_code") {
		return [
			...shared,
			'- When the user requests a Claude subagent or a Claude model family, use Claude\'s native Agent tool. Set Agent.model to the matching family alias: "opus", "sonnet", or "haiku". For example, "Opus 5" means Agent.model = "opus".',
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

	const lines = buildNativeSubagentInstructions(currentProvider);
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
		"Dev Command Center tool: delegate_task",
		"You may ask the human to delegate a bounded subtask to another provider by emitting a DCC permission request.",
		"Use it only when another provider can provide materially useful review, explanation, or implementation help.",
		"Emit exactly this JSON event through the provider permission channel:",
		JSON.stringify({
			type: "dcc_permission_request",
			request_id: "delegate-task-short-id",
			tool_name: "delegate_task",
			title: "Delegate task",
			description: "One sentence explaining why delegation is useful.",
			command: JSON.stringify({
				instruction: "Specific task for the delegated provider.",
				mode: "review",
				contextPolicy: "review_current_diff",
				targetProviderId: targets[0]?.id ?? null,
			}),
		}),
		"Allowed modes: review, explain, implement. Use implement only when file edits are necessary; DCC will require human review before completion.",
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
