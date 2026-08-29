import type { PendingAnnotation } from "./diff-annotation";

export type WorkspaceDiffAnnotationCommand = {
	workspaceId: string;
	pending: PendingAnnotation;
	targetSessionId?: string | null;
};

const WORKSPACE_DIFF_ANNOTATION_EVENT = "dcc:workspace-diff-annotation";

export function dispatchWorkspaceDiffAnnotation(
	command: WorkspaceDiffAnnotationCommand,
): void {
	window.dispatchEvent(
		new CustomEvent<WorkspaceDiffAnnotationCommand>(
			WORKSPACE_DIFF_ANNOTATION_EVENT,
			{ detail: command },
		),
	);
}

export function subscribeWorkspaceDiffAnnotation(
	listener: (command: WorkspaceDiffAnnotationCommand) => void,
): () => void {
	const handler = (event: Event) => {
		listener(
			(event as CustomEvent<WorkspaceDiffAnnotationCommand>).detail,
		);
	};
	window.addEventListener(WORKSPACE_DIFF_ANNOTATION_EVENT, handler);
	return () =>
		window.removeEventListener(WORKSPACE_DIFF_ANNOTATION_EVENT, handler);
}
