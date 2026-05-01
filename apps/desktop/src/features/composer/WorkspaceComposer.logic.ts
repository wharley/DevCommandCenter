export const composerToolbarTriggerClassName =
	"cursor-pointer rounded-[9px] px-1 py-0.5 text-[13px] font-medium transition-colors hover:bg-accent/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/50";

export function getComposerDraftKey(workspaceId: string) {
	return `dcc.workspace.composer.draft.${workspaceId}`;
}

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
