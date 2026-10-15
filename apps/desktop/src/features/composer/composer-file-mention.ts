/** A worktree file to mention in the conversation's composer, from outside it. */
export type ComposerFileMention = { path: string };

const MENTION_FILE_EVENT = "dcc:composer-mention-file";

/** Drag type for a worktree file dragged from the file tree. */
export const WORKTREE_FILE_DRAG_TYPE = "application/x-dcc-worktree-file";

export function mentionFileInComposer(mention: ComposerFileMention): void {
	window.dispatchEvent(
		new CustomEvent<ComposerFileMention>(MENTION_FILE_EVENT, { detail: mention }),
	);
}

export function subscribeComposerFileMention(
	listener: (mention: ComposerFileMention) => void,
): () => void {
	const handler = (event: Event) =>
		listener((event as CustomEvent<ComposerFileMention>).detail);
	window.addEventListener(MENTION_FILE_EVENT, handler);
	return () => window.removeEventListener(MENTION_FILE_EVENT, handler);
}
