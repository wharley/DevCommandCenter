import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown } from "lucide-react";
import { Badge } from "@/components/ui/badge";
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

function eventTone(event: CoreEvent): "outline" | "secondary" | "success" | "warn" {
	if ("sessionCompleted" in event || "workspaceReady" in event) {
		return "success";
	}
	if ("sessionAborted" in event || "sessionTurnAborted" in event) {
		return "warn";
	}
	if ("sessionCheckpointCreated" in event) {
		return "warn";
	}
	if ("sessionStarted" in event || "sessionResumed" in event) {
		return "secondary";
	}
	return "outline";
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

	const timelineRows =
		events.length === 0 ? (
			<div className="flex min-h-full flex-1 items-center justify-center px-8">
				<p className="m-0 max-w-md text-center text-[13px] leading-relaxed text-muted-foreground">
					No messages yet — start a session and send something. Session events appear in this
					timeline.
				</p>
			</div>
		) : (
			<ul className="m-0 list-none p-0">
				{events.map((event, index) => (
					<li
						key={`${eventLabel(event)}-${index}`}
						className="flow-root px-5 pb-1.5 [content-visibility:auto]"
					>
						<div className="dcc-runtime-feed__row dcc-session-event" data-tone={eventTone(event)}>
							<div className="dcc-session-event__header">
								<Badge variant={eventTone(event)} className="font-normal">
									{eventLabel(event)}
								</Badge>
							</div>
							<p className="dcc-session-event__copy text-[13px] leading-snug">
								{eventPayloadSummary(event)}
							</p>
						</div>
					</li>
				))}
			</ul>
		);

	if (compact) {
		return (
			<div className="dcc-conversation-scroll-area relative min-h-0 flex-1 overflow-hidden">
				<div
					ref={scrollRef}
					className="dcc-conversation-scroll-viewport h-full w-full overflow-x-hidden overflow-y-auto overscroll-none"
				>
					<div className="flex min-h-full min-w-0 flex-col">
						<div className="h-6 shrink-0" aria-hidden />
						{timelineRows}
					</div>
				</div>
				{showScrollToLatest ? (
					<div className="pointer-events-none absolute bottom-1 left-1/2 z-30 flex -translate-x-1/2 justify-center py-1.5">
						<button
							type="button"
							onClick={() => {
								const container = scrollRef.current;
								if (!container) return;
								container.scrollTo({
									top: container.scrollHeight,
									behavior: "smooth",
								});
							}}
							className="pointer-events-auto flex items-center gap-1.5 rounded-full border border-border/60 bg-card px-3 py-1 text-xs text-muted-foreground shadow-sm transition-colors hover:border-border hover:text-foreground hover:cursor-pointer"
						>
							<ChevronDown className="size-3.5" strokeWidth={2} />
							Scroll to bottom
						</button>
					</div>
				) : null}
			</div>
		);
	}

	return (
		<Card className="dcc-runtime-feed">
			<CardHeader>
				<div className="dcc-card__meta-row">
					<CardTitle>Session events</CardTitle>
					<Badge variant="outline">{events.length}</Badge>
				</div>
			</CardHeader>
			<CardContent className="dcc-runtime-feed__content">{timelineRows}</CardContent>
		</Card>
	);
}
