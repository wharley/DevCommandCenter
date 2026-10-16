import type { BrowserOpenRequest } from "./browser-api";

/**
 * Approved Browser opens waiting for their task's surface, one per task. A
 * second approval for another task must not erase the first: each task opens
 * its own request when the person reaches it.
 */
export type BrowserOpenQueue = Readonly<Record<string, BrowserOpenRequest>>;

const taskKey = (workspaceId: string, sessionId: string) => `${workspaceId}\u001f${sessionId}`;

/** A newer approval for the same task replaces the older one. */
export function enqueueBrowserOpen(queue: BrowserOpenQueue, request: BrowserOpenRequest): BrowserOpenQueue {
	return { ...queue, [taskKey(request.workspaceId, request.sessionId)]: request };
}

export function removeBrowserOpen(queue: BrowserOpenQueue, requestId: string): BrowserOpenQueue {
	const entry = Object.entries(queue).find(([, request]) => request.requestId === requestId);
	if (!entry) return queue;
	const { [entry[0]]: _removed, ...rest } = queue;
	return rest;
}

export function browserOpenForTask(
	queue: BrowserOpenQueue,
	workspaceId: string | null | undefined,
	sessionId: string | null | undefined,
): BrowserOpenRequest | null {
	if (!workspaceId || !sessionId) return null;
	return queue[taskKey(workspaceId, sessionId)] ?? null;
}

export function hasBrowserOpen(queue: BrowserOpenQueue, requestId: string): boolean {
	return Object.values(queue).some((request) => request.requestId === requestId);
}
