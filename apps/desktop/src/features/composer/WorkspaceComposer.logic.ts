import type { CoreEvent } from "@dcc/contracts";

export const composerToolbarTriggerClassName =
	"cursor-pointer rounded-[9px] px-1 py-0.5 text-[13px] font-medium transition-colors hover:bg-accent/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/50";

export function getCompactComposerModelLabel(
	providerId: string | null,
	modelLabel: string,
) {
	const normalized = modelLabel.trim();
	if (providerId === "claude_code") {
		return normalized.replace(/^Claude\s+/i, "") || normalized;
	}
	return normalized;
}

export type PlanModeStateMap = Record<string, boolean>;

export function getComposerDraftKey(workspaceId: string) {
	return `dcc.workspace.composer.draft.${workspaceId}`;
}

/** Draft persistence key isolated to one conversation within a workspace. */
export function getComposerConversationDraftKey(
	workspaceId: string,
	sessionId: string | null,
) {
	const conversationKey = sessionId?.trim()
		? `session.${encodeURIComponent(sessionId.trim())}`
		: "new";
	return `${getComposerDraftKey(workspaceId)}.conversation.${conversationKey}`;
}

export function getComposerEffortKey(workspaceId: string) {
	return `dcc.workspace.composer.effort.${workspaceId}`;
}

export function getComposerApprovalPolicyKey(
	workspaceId: string,
	providerId: string | null,
) {
	return `dcc.workspace.composer.approval.${workspaceId}.${providerId ?? "provider-managed"}`;
}

export function shouldApplyComposerPrefill(
	lastAppliedRequestId: string | null,
	prefill: { requestId: string; text: string } | null | undefined,
) {
	return Boolean(
		prefill &&
			prefill.text.length > 0 &&
			prefill.requestId !== lastAppliedRequestId,
	);
}

function getWorkspacePlanModeScopeKey(workspaceId: string | null) {
	return workspaceId ? `workspace:${workspaceId}` : null;
}

function getSessionPlanModeScopeKey(sessionId: string | null) {
	return sessionId ? `session:${sessionId}` : null;
}

export function resolvePlanModeState(
	stateMap: PlanModeStateMap,
	input: {
		workspaceId: string | null;
		sessionId: string | null;
	},
) {
	const sessionKey = getSessionPlanModeScopeKey(input.sessionId);
	if (sessionKey && sessionKey in stateMap) {
		return stateMap[sessionKey] === true;
	}

	const workspaceKey = getWorkspacePlanModeScopeKey(input.workspaceId);
	if (workspaceKey && workspaceKey in stateMap) {
		return stateMap[workspaceKey] === true;
	}

	return false;
}

export function setPlanModeState(
	stateMap: PlanModeStateMap,
	input: {
		workspaceId: string | null;
		sessionId: string | null;
		enabled: boolean;
	},
) {
	const next = { ...stateMap };
	const workspaceKey = getWorkspacePlanModeScopeKey(input.workspaceId);
	const sessionKey = getSessionPlanModeScopeKey(input.sessionId);

	if (workspaceKey) {
		next[workspaceKey] = input.enabled;
	}

	if (sessionKey) {
		next[sessionKey] = input.enabled;
	}

	return next;
}

/** Shared submit-state helpers for send vs steer behavior. */
export type ComposerSendDecision =
	| { kind: "send" }
	| { kind: "steer" }
	| { kind: "blocked"; reason: "empty" | "disabled" | "queued" };

export function canSendPrompt({
	disabled,
	hasContent,
	isSubmitting,
}: {
	disabled: boolean;
	hasContent: boolean;
	isSubmitting: boolean;
}) {
	return !disabled && hasContent && !isSubmitting;
}

export function decideSend({
	hasContent,
	sending,
	disabled,
}: {
	hasContent: boolean;
	sending: boolean;
	disabled: boolean;
}): ComposerSendDecision {
	if (disabled) {
		return { kind: "blocked", reason: "disabled" };
	}
	if (!hasContent) {
		return { kind: "blocked", reason: "empty" };
	}
	if (sending) {
		return { kind: "steer" };
	}
	return { kind: "send" };
}

/** Shared gate for Send, Steer, and ⌘Enter. */
export function isComposerSubmitEnabled({
	disabled,
	hasProvider,
	hasContent,
}: {
	disabled: boolean;
	hasProvider: boolean;
	hasContent: boolean;
}) {
	return !disabled && hasProvider && hasContent;
}

export function isSendDisabled(submitEnabled: boolean, sending: boolean) {
	return !submitEnabled || sending;
}

export function isSteerDisabled(submitEnabled: boolean, sending: boolean) {
	return !submitEnabled || !sending;
}

/**
 * Returns a stable key for the latest live event that changes the durable turn
 * queue. Token and timeline events intentionally reuse the previous key so they
 * do not poll the queue while an agent is streaming.
 */
export function latestTurnQueueEventKey(events: CoreEvent[]) {
	for (let index = events.length - 1; index >= 0; index -= 1) {
		const event = events[index];
		if ("sessionTurnQueued" in event && event.sessionTurnQueued) {
			const value = event.sessionTurnQueued;
			return `${value.session_id}:queued:${value.queued_turn.id}`;
		}
		if ("sessionQueuedTurnRemoved" in event && event.sessionQueuedTurnRemoved) {
			const value = event.sessionQueuedTurnRemoved;
			return `${value.session_id}:removed:${value.queued_turn_id}`;
		}
		if ("sessionTurnQueueReordered" in event && event.sessionTurnQueueReordered) {
			const value = event.sessionTurnQueueReordered;
			return `${value.session_id}:reordered:${value.queued_turn_ids.join(",")}`;
		}
		if (
			"sessionQueuedTurnDispatched" in event &&
			event.sessionQueuedTurnDispatched
		) {
			const value = event.sessionQueuedTurnDispatched;
			return `${value.session_id}:dispatched:${value.queued_turn_id}:${value.turn_id}`;
		}
	}
	return null;
}

export async function submitComposerDraftOptimistically({
	clearSubmittedDraft,
	submit,
	restoreSubmittedDraft,
}: {
	clearSubmittedDraft: () => void;
	submit: () => Promise<boolean>;
	restoreSubmittedDraft: () => void;
}) {
	clearSubmittedDraft();
	try {
		const accepted = await submit();
		if (!accepted) {
			restoreSubmittedDraft();
		}
		return accepted;
	} catch {
		restoreSubmittedDraft();
		return false;
	}
}

export function buildMissionSpecFilename(workspaceBranch: string | null) {
	const source = workspaceBranch?.trim() || "mission";
	const slug = source
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+|-+$/g, "");
	return `${slug || "mission"}.spec.md`;
}

export function buildSpecDraftPrompt({
	workspaceBranch,
}: {
	workspaceBranch: string | null;
}) {
	const specPath = `.devcommandcenter/specs/${buildMissionSpecFilename(workspaceBranch)}`;
	return [
		"Create or update the DCC mission spec for this worktree.",
		"",
		`Spec path: ${specPath}`,
		"Template: .devcommandcenter/spec.template.md",
		"",
		"Use the request below as source material. If it is insufficient, ask concise clarifying questions. Do not implement code yet; stop after the spec is written or the questions are asked.",
		"",
		"Request:",
	].join("\n");
}
