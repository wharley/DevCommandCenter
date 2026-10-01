const CALL_AGENT_EVENT = "dcc:call-agent";

/** Asks the app shell to start a session of this agent in the current task. */
export function dispatchCallAgent(agentId: string): void {
	window.dispatchEvent(new CustomEvent<string>(CALL_AGENT_EVENT, { detail: agentId }));
}

export function subscribeCallAgent(listener: (agentId: string) => void): () => void {
	const handler = (event: Event) => listener((event as CustomEvent<string>).detail);
	window.addEventListener(CALL_AGENT_EVENT, handler);
	return () => window.removeEventListener(CALL_AGENT_EVENT, handler);
}
