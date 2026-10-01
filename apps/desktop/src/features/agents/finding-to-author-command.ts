import type { DiffMachineAnnotation } from "@/features/editor/diff-types";

/** A reviewer finding the person wants the task's author agent to fix. */
export type FindingToAuthorCommand = {
	workspaceId: string;
	path: string;
	startLine: number;
	endLine: number;
	title: string;
	snippet: string;
};

const FINDING_TO_AUTHOR_EVENT = "dcc:agent-finding-to-author";

/** The lines a finding points at, as they are in the file now. */
export function findingSnippet(text: string, startLine: number, endLine: number): string {
	return text
		.split(/\r?\n/)
		.slice(Math.max(0, startLine - 1), Math.max(startLine, endLine))
		.join("\n");
}

/**
 * Hands a clicked reviewer marking to the task panel, which drafts a fix
 * request in the author agent's composer. Other markings are ignored.
 */
export function dispatchFindingToAuthor(input: {
	workspaceId: string | null | undefined;
	path: string;
	annotation: DiffMachineAnnotation;
	/** The whole current file, or the marked lines when only a patch is at hand. */
	modifiedText?: string;
	snippet?: string;
}): void {
	const { workspaceId, path, annotation, modifiedText } = input;
	if (!workspaceId || annotation.source !== "agent-review") {
		return;
	}
	window.dispatchEvent(
		new CustomEvent<FindingToAuthorCommand>(FINDING_TO_AUTHOR_EVENT, {
			detail: {
				workspaceId,
				path,
				startLine: annotation.startLine,
				endLine: annotation.endLine,
				title: annotation.title,
				snippet:
					input.snippet ??
					findingSnippet(modifiedText ?? "", annotation.startLine, annotation.endLine),
			},
		}),
	);
}

export function subscribeFindingToAuthor(
	listener: (command: FindingToAuthorCommand) => void,
): () => void {
	const handler = (event: Event) => listener((event as CustomEvent<FindingToAuthorCommand>).detail);
	window.addEventListener(FINDING_TO_AUTHOR_EVENT, handler);
	return () => window.removeEventListener(FINDING_TO_AUTHOR_EVENT, handler);
}

type SessionLike = {
	session: { id: string };
	lastTurnStartedAt?: string | null;
};

/**
 * The session that wrote the changes: the plain (non-agent) session of the
 * task the person used most recently.
 */
export function authorSessionId(
	sessions: SessionLike[],
	isAgentSession: (sessionId: string) => boolean,
): string | null {
	const plain = sessions.filter((summary) => !isAgentSession(summary.session.id));
	const latest = plain.reduce<SessionLike | null>(
		(best, summary) =>
			!best || (summary.lastTurnStartedAt ?? "") > (best.lastTurnStartedAt ?? "") ? summary : best,
		null,
	);
	return latest?.session.id ?? null;
}
