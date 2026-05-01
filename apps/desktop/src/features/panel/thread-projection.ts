import type { CoreEvent } from "@dcc/contracts";

export type WorkspaceMessageRole = "user" | "assistant" | "system";

export type WorkspaceMessage = {
	id: string;
	role: WorkspaceMessageRole;
	content: string;
	label: string;
	streaming?: boolean;
};

function eventLabel(event: CoreEvent): string {
	if ("sessionStarted" in event) return "session.started";
	if ("sessionCompleted" in event) return "session.completed";
	if ("sessionAborted" in event) return "session.aborted";
	if ("sessionResumed" in event) return "session.resumed";
	if ("sessionTurnStarted" in event) return "session.turn.started";
	if ("sessionTurnDelta" in event) return "session.turn.delta";
	if ("sessionTurnCompleted" in event) return "session.turn.completed";
	if ("sessionTurnAborted" in event) return "session.turn.aborted";
	if ("sessionCheckpointCreated" in event) return "session.checkpoint.created";
	if ("workspacePrepared" in event) return "workspace.prepared";
	if ("workspaceReady" in event) return "workspace.ready";
	return "event";
}

function eventSummary(event: CoreEvent): string {
	if ("sessionStarted" in event && event.sessionStarted) {
		return `${event.sessionStarted.session_id} · ${event.sessionStarted.provider_id}`;
	}
	if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
		return event.sessionTurnStarted.prompt;
	}
	if ("sessionTurnDelta" in event && event.sessionTurnDelta) {
		return event.sessionTurnDelta.content;
	}
	if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
		return event.sessionTurnCompleted.turn_id;
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return event.sessionTurnAborted.reason ?? "Turn aborted";
	}
	if ("sessionCheckpointCreated" in event && event.sessionCheckpointCreated) {
		return event.sessionCheckpointCreated.label;
	}
	if ("workspacePrepared" in event && event.workspacePrepared) {
		return `${event.workspacePrepared.project_id} · ${event.workspacePrepared.worktree_path}`;
	}
	if ("workspaceReady" in event && event.workspaceReady) {
		return `${event.workspaceReady.project_id} · ${event.workspaceReady.worktree_path}`;
	}
	if ("sessionCompleted" in event && event.sessionCompleted) {
		return event.sessionCompleted.session_id;
	}
	if ("sessionAborted" in event && event.sessionAborted) {
		return event.sessionAborted.reason ?? "Session aborted";
	}
	if ("sessionResumed" in event && event.sessionResumed) {
		return event.sessionResumed.session_id;
	}
	return "No payload summary";
}

export function projectWorkspaceMessages(events: CoreEvent[]): WorkspaceMessage[] {
	const messages: WorkspaceMessage[] = [];
	const assistantBuckets = new Map<string, WorkspaceMessage>();

	for (const event of events) {
		if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
			messages.push({
				id: `user-${event.sessionTurnStarted.session_id}-${event.sessionTurnStarted.turn_id}`,
				role: "user",
				label: "User",
				content: event.sessionTurnStarted.prompt,
			});
			continue;
		}

		if ("sessionTurnDelta" in event && event.sessionTurnDelta) {
			const key = event.sessionTurnDelta.turn_id;
			const existing = assistantBuckets.get(key);
			if (existing) {
				existing.content = `${existing.content}${event.sessionTurnDelta.content}`;
			} else {
				const message: WorkspaceMessage = {
					id: `assistant-${event.sessionTurnDelta.session_id}-${event.sessionTurnDelta.turn_id}`,
					role: "assistant",
					label: "Assistant",
					content: event.sessionTurnDelta.content,
					streaming: true,
				};
				assistantBuckets.set(key, message);
				messages.push(message);
			}
			continue;
		}

		if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
			const key = event.sessionTurnCompleted.turn_id;
			const existing = assistantBuckets.get(key);
			if (existing) {
				existing.streaming = false;
			}
			continue;
		}

		messages.push({
			id: `${eventLabel(event)}-${messages.length}`,
			role: "system",
			label: eventLabel(event),
			content: eventSummary(event),
		});
	}

	return messages;
}
