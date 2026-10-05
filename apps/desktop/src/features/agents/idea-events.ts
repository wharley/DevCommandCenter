const IDEA_FINISHED_EVENT = "dcc:idea-finished";

/** An idea left ~/dcc-ideias: published as a project, or discarded. */
export type IdeaFinished =
	| { kind: "published"; ideaWorkspaceId: string; rootPath: string }
	| { kind: "discarded"; ideaWorkspaceId: string };

/** Tells the app shell where to go next; the idea panel lives inside the task. */
export function dispatchIdeaFinished(detail: IdeaFinished): void {
	window.dispatchEvent(new CustomEvent<IdeaFinished>(IDEA_FINISHED_EVENT, { detail }));
}

export function subscribeIdeaFinished(listener: (detail: IdeaFinished) => void): () => void {
	const handler = (event: Event) => listener((event as CustomEvent<IdeaFinished>).detail);
	window.addEventListener(IDEA_FINISHED_EVENT, handler);
	return () => window.removeEventListener(IDEA_FINISHED_EVENT, handler);
}
