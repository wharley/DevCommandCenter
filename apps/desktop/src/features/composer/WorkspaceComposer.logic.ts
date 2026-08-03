export const composerToolbarTriggerClassName =
	"cursor-pointer rounded-[9px] px-1 py-0.5 text-[13px] font-medium transition-colors hover:bg-accent/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/50";

export type PlanModeStateMap = Record<string, boolean>;

export function getComposerDraftKey(workspaceId: string) {
	return `dcc.workspace.composer.draft.${workspaceId}`;
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
