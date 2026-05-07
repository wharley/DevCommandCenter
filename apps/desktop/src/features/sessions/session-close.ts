import type { WorkspaceSessionSummary } from "@dcc/contracts";

export function isSessionArchived(summary: WorkspaceSessionSummary): boolean {
	return summary.thread.archived_at != null;
}

export function isSessionEmpty(summary: WorkspaceSessionSummary): boolean {
	return summary.projection.turnCount === 0;
}

export function visibleSessions(
	sessions: WorkspaceSessionSummary[],
): WorkspaceSessionSummary[] {
	return sessions.filter((summary) => !isSessionArchived(summary));
}

export function nextVisibleSessionIdAfterClose(
	sessions: WorkspaceSessionSummary[],
	closingSessionId: string,
): string | null {
	return (
		visibleSessions(sessions).find((summary) => summary.session.id !== closingSessionId)
			?.session.id ?? null
	);
}

export function shouldCreateReplacementSession(
	sessions: WorkspaceSessionSummary[],
	closingSessionId: string,
): boolean {
	return nextVisibleSessionIdAfterClose(sessions, closingSessionId) == null;
}
