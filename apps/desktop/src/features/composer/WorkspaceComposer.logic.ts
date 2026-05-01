export const composerToolbarTriggerClassName =
	"cursor-pointer rounded-[9px] px-1 py-0.5 text-[13px] font-medium transition-colors hover:bg-accent/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/50";

export function getComposerDraftKey(workspaceId: string) {
	return `dcc.workspace.composer.draft.${workspaceId}`;
}

export type ComposerDecision =
	| { kind: "send" }
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
}): ComposerDecision {
	if (disabled) {
		return { kind: "blocked", reason: "disabled" };
	}

	if (!hasContent) {
		return { kind: "blocked", reason: "empty" };
	}

	return sending ? { kind: "blocked", reason: "queued" } : { kind: "send" };
}

export type ComposerContextDirectory = {
	id: string;
	label: string;
	path: string;
};

export function buildComposerContextDirectories({
	workspacePath,
	workspaceBranch,
}: {
	workspacePath: string | null;
	workspaceBranch: string | null;
}): ComposerContextDirectory[] {
	const directories: ComposerContextDirectory[] = [];

	if (workspacePath) {
		directories.push({
			id: "workspace-path",
			label: "workspace",
			path: workspacePath,
		});
	}

	if (workspaceBranch) {
		directories.push({
			id: "workspace-branch",
			label: "branch",
			path: workspaceBranch,
		});
	}

	return directories;
}
