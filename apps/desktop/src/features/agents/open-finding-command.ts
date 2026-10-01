/** Opens a changed file's diff scrolled to a reviewer finding. */
export type OpenFindingCommand = { workspaceId: string; path: string; line: number };

const OPEN_FINDING_EVENT = "dcc:open-reviewer-finding";

export function dispatchOpenFinding(command: OpenFindingCommand): void {
	window.dispatchEvent(new CustomEvent<OpenFindingCommand>(OPEN_FINDING_EVENT, { detail: command }));
}

export function subscribeOpenFinding(listener: (command: OpenFindingCommand) => void): () => void {
	const handler = (event: Event) => listener((event as CustomEvent<OpenFindingCommand>).detail);
	window.addEventListener(OPEN_FINDING_EVENT, handler);
	return () => window.removeEventListener(OPEN_FINDING_EVENT, handler);
}
