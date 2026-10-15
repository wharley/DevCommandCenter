import type { Delegation } from "@dcc/contracts";

/** What a child task row says about where the task stands. */
/**
 * `working` has a turn in progress; `idle` is open but stopped between turns;
 * `waiting` is stopped on the person (approval, question).
 */
export type ChildTaskState = "proposed" | "working" | "idle" | "waiting" | "review";

export type ChildTaskEntry = {
	id: string;
	title: string;
	state: ChildTaskState;
	/** The conversation a click opens: the task's own, or the parent's while it is only a proposal. */
	sessionId: string;
};

const STATE_BY_STATUS: Partial<Record<Delegation["status"], ChildTaskState>> = {
	draft: "proposed",
	queued: "idle",
	running: "idle",
	review_pending: "review",
};

/**
 * The tasks still in play under each workspace's sidebar row, oldest first,
 * as the plan's phases were proposed. Closed, failed and dismissed tasks
 * leave the sidebar; plain delegations never enter it.
 */
export function childTasksByWorkspace(delegations: readonly Delegation[]): Map<string, ChildTaskEntry[]> {
	const byWorkspace = new Map<string, ChildTaskEntry[]>();
	const ordered = [...delegations].sort((left, right) => left.createdAt.localeCompare(right.createdAt));
	for (const delegation of ordered) {
		const state = STATE_BY_STATUS[delegation.status];
		if (delegation.kind !== "task" || !state) continue;
		const entry: ChildTaskEntry = {
			id: delegation.id,
			title: delegation.title?.trim() || delegation.instruction?.trim().split("\n")[0] || delegation.id,
			state,
			sessionId: delegation.childSessionId ?? delegation.parentSessionId,
		};
		const list = byWorkspace.get(delegation.workspaceId);
		if (list) list.push(entry);
		else byWorkspace.set(delegation.workspaceId, [entry]);
	}
	return byWorkspace;
}

/** What each open task is doing right now, from its conversation's live turn. */
export function withLiveTaskState(
	tasks: readonly ChildTaskEntry[],
	live: { runningSessionIds?: readonly string[]; waitingSessionIds?: readonly string[] } | null | undefined,
): ChildTaskEntry[] {
	return tasks.map((task) => {
		if (task.state !== "idle") return task;
		if (live?.waitingSessionIds?.includes(task.sessionId)) return { ...task, state: "waiting" };
		if (live?.runningSessionIds?.includes(task.sessionId)) return { ...task, state: "working" };
		return task;
	});
}
