import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowDown } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { CoreEvent } from "@dcc/contracts";

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
	const sessionTurnDelta =
		"sessionTurnDelta" in event ? event.sessionTurnDelta : null;
	if (sessionTurnDelta) {
		const preview =
			sessionTurnDelta.content.length > 60
				? `${sessionTurnDelta.content.slice(0, 60)}…`
				: sessionTurnDelta.content;
		return `${sessionTurnDelta.session_id} · ${preview}`;
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

export function SessionEventFeed({
	events,
	compact = false,
}: {
	events: CoreEvent[];
	compact?: boolean;
}) {
	const scrollRef = useRef<HTMLDivElement | null>(null);
	const [showScrollToLatest, setShowScrollToLatest] = useState(false);
	const latestEventKey = useMemo(() => {
		const last = events[events.length - 1];
		return last ? eventLabel(last) : "empty";
	}, [events]);

	useEffect(() => {
		const container = scrollRef.current;
		if (!container || !compact) {
			return;
		}

		container.scrollTop = container.scrollHeight;
	}, [compact, latestEventKey]);

	useEffect(() => {
		if (!compact) {
			return;
		}

		const container = scrollRef.current;
		if (!container) {
			return;
		}

		const updateVisibility = () => {
			const remaining = container.scrollHeight - container.scrollTop - container.clientHeight;
			setShowScrollToLatest(remaining > 24);
		};

		updateVisibility();
		container.addEventListener("scroll", updateVisibility, { passive: true });
		window.addEventListener("resize", updateVisibility);
		return () => {
			container.removeEventListener("scroll", updateVisibility);
			window.removeEventListener("resize", updateVisibility);
		};
	}, [compact, latestEventKey]);

	const content = (
		<>
			{events.length === 0 ? (
				<div className="dcc-session-thread-empty">
					<p className="dcc-card__description">
						No session events yet. Start a thread to see the Tauri listen bridge
						in action.
					</p>
				</div>
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
		</>
	);

	if (compact) {
		return (
			<div className="dcc-session-timeline">
				<div className="dcc-card__meta-row dcc-session-timeline__header">
					<div>
						<CardTitle>Session events</CardTitle>
						<p className="dcc-card__description">
							Live stream from the Tauri listen bridge.
						</p>
					</div>
					<Badge variant="outline">{events.length} recent</Badge>
				</div>
				<div ref={scrollRef} className="dcc-session-timeline__content">
					<div className="dcc-session-timeline__surface">{content}</div>
				</div>
				<Button
					type="button"
					variant="secondary"
					size="icon-sm"
					className={`dcc-session-timeline__scroll ${showScrollToLatest ? "is-visible" : ""}`}
					onClick={() => {
						const container = scrollRef.current;
						if (!container) return;
						container.scrollTo({
							top: container.scrollHeight,
							behavior: "smooth",
						});
					}}
					aria-label="Scroll to latest event"
				>
					<ArrowDown />
				</Button>
			</div>
		);
	}

	return (
		<Card className="dcc-runtime-feed">
			<CardHeader>
				<div className="dcc-card__meta-row">
					<CardTitle>Session events</CardTitle>
					<Badge variant="outline">{events.length} recent</Badge>
				</div>
			</CardHeader>
			<CardContent className="dcc-runtime-feed__content">{content}</CardContent>
		</Card>
	);
}
