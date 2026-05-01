import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { CoreEvent } from "@dcc/contracts";

function eventLabel(event: CoreEvent): string {
	if ("sessionStarted" in event) return "session.started";
	if ("sessionCompleted" in event) return "session.completed";
	if ("sessionAborted" in event) return "session.aborted";
	if ("sessionResumed" in event) return "session.resumed";
	if ("sessionTurnStarted" in event) return "session.turn.started";
	if ("sessionTurnCompleted" in event) return "session.turn.completed";
	if ("sessionTurnAborted" in event) return "session.turn.aborted";
	if ("sessionCheckpointCreated" in event) return "session.checkpoint.created";
	if ("workspacePrepared" in event) return "workspace.prepared";
	if ("workspaceReady" in event) return "workspace.ready";
	return "event";
}

function eventPayloadSummary(event: CoreEvent): string {
	const sessionStarted = "sessionStarted" in event ? event.sessionStarted : null;
	if (sessionStarted) {
		return `${sessionStarted.session_id} · ${sessionStarted.provider_id}`;
	}
	const sessionTurnStarted =
		"sessionTurnStarted" in event ? event.sessionTurnStarted : null;
	if (sessionTurnStarted) {
		return `${sessionTurnStarted.session_id} · ${sessionTurnStarted.prompt}`;
	}
	const sessionTurnAborted =
		"sessionTurnAborted" in event ? event.sessionTurnAborted : null;
	if (sessionTurnAborted) {
		return `${sessionTurnAborted.session_id} · ${sessionTurnAborted.reason ?? "no reason"}`;
	}
	const sessionCheckpointCreated =
		"sessionCheckpointCreated" in event ? event.sessionCheckpointCreated : null;
	if (sessionCheckpointCreated) {
		return `${sessionCheckpointCreated.session_id} · ${sessionCheckpointCreated.label}`;
	}
	if ("workspacePrepared" in event || "workspaceReady" in event) {
		const payload = "workspacePrepared" in event ? event.workspacePrepared : event.workspaceReady;
		return payload
			? `${payload.project_id} · ${payload.worktree_path}`
			: "No payload summary";
	}
	const sessionCompleted =
		"sessionCompleted" in event ? event.sessionCompleted : null;
	if (sessionCompleted) {
		return sessionCompleted.session_id;
	}
	const sessionAborted = "sessionAborted" in event ? event.sessionAborted : null;
	if (sessionAborted) {
		return `${sessionAborted.session_id} · ${sessionAborted.reason ?? "no reason"}`;
	}
	const sessionResumed = "sessionResumed" in event ? event.sessionResumed : null;
	if (sessionResumed) {
		return sessionResumed.session_id;
	}
	const sessionTurnCompleted =
		"sessionTurnCompleted" in event ? event.sessionTurnCompleted : null;
	if (sessionTurnCompleted) {
		return `${sessionTurnCompleted.session_id} · ${sessionTurnCompleted.turn_id}`;
	}
	return "No payload summary";
}

export function SessionEventFeed({ events }: { events: CoreEvent[] }) {
	return (
		<Card className="dcc-runtime-feed">
			<CardHeader>
				<div className="dcc-card__meta-row">
					<CardTitle>Session events</CardTitle>
					<Badge variant="outline">{events.length} recent</Badge>
				</div>
			</CardHeader>
			<CardContent className="dcc-runtime-feed__content">
				{events.length === 0 ? (
					<p className="dcc-card__description">
						No session events yet. Start a thread to see the Tauri listen bridge
						in action.
					</p>
				) : (
					<ul className="dcc-runtime-feed__list">
						{events.map((event, index) => (
							<li key={`${eventLabel(event)}-${index}`}>
								<div className="dcc-runtime-feed__row">
									<strong>{eventLabel(event)}</strong>
									<small>{eventPayloadSummary(event)}</small>
								</div>
							</li>
						))}
					</ul>
				)}
			</CardContent>
		</Card>
	);
}
