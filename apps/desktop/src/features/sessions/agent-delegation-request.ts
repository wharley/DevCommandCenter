import type { DelegationContextPolicy, DelegationMode } from "@dcc/contracts";

export type AgentInitiatedDelegationRequest = {
	instruction: string;
	mode: Extract<DelegationMode, "review" | "explain" | "implement">;
	contextPolicy: DelegationContextPolicy;
	targetProviderId: string | null;
	targetModelId: string | null;
};

type DelegateTaskPayload = {
	instruction?: unknown;
	task?: unknown;
	prompt?: unknown;
	mode?: unknown;
	contextPolicy?: unknown;
	context_policy?: unknown;
	targetProviderId?: unknown;
	target_provider_id?: unknown;
	targetModelId?: unknown;
	target_model_id?: unknown;
};

function readString(value: unknown): string | null {
	return typeof value === "string" && value.trim().length > 0 ? value.trim() : null;
}

function parseMode(value: unknown): AgentInitiatedDelegationRequest["mode"] {
	if (value === "explain" || value === "implement") {
		return value;
	}
	return "review";
}

function parseContextPolicy(value: unknown): DelegationContextPolicy {
	if (typeof value === "object" && value && "type" in value) {
		const type = (value as { type?: unknown }).type;
		if (
			type === "minimal" ||
			type === "review_current_diff" ||
			type === "full_reanchor"
		) {
			return { type };
		}
	}
	if (value === "minimal" || value === "review_current_diff" || value === "full_reanchor") {
		return { type: value };
	}
	return { type: "review_current_diff" };
}

function parsePayload(raw: string | null | undefined): DelegateTaskPayload {
	if (!raw?.trim()) {
		return {};
	}
	try {
		const parsed = JSON.parse(raw);
		return typeof parsed === "object" && parsed !== null ? parsed : {};
	} catch {
		return { instruction: raw };
	}
}

export function parseAgentInitiatedDelegationRequest(input: {
	command?: string;
	description?: string;
}): AgentInitiatedDelegationRequest | null {
	const payload = parsePayload(input.command);
	const instruction =
		readString(payload.instruction) ??
		readString(payload.task) ??
		readString(payload.prompt) ??
		readString(input.description);
	if (!instruction) {
		return null;
	}

	return {
		instruction,
		mode: parseMode(payload.mode),
		contextPolicy: parseContextPolicy(payload.contextPolicy ?? payload.context_policy),
		targetProviderId:
			readString(payload.targetProviderId) ?? readString(payload.target_provider_id),
		targetModelId: readString(payload.targetModelId) ?? readString(payload.target_model_id),
	};
}

export function isDelegateTaskTool(toolName: string) {
	return toolName.trim().toLowerCase() === "delegate_task";
}
