/** The session catalog can update before startThread returns its session ID. */
export function visibleSessionPendingPrompt(input: {
	prompt: string | null;
	pendingSessionId: string | null;
	startingWorkspaceId: string | null;
	workspaceId: string | null;
	sessionId: string | null;
}): string | null {
	if (input.pendingSessionId) {
		return input.pendingSessionId === input.sessionId ? input.prompt : null;
	}
	return input.startingWorkspaceId && input.startingWorkspaceId === input.workspaceId
		? input.prompt
		: null;
}
