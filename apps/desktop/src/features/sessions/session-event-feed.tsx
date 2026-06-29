import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { CoreEvent } from "@dcc/contracts";

/** Session id that an event belongs to, or null for non-session events. */
function eventSessionId(event: CoreEvent): string | null {
	const payload =
		("sessionStarted" in event && event.sessionStarted) ||
		("sessionCompleted" in event && event.sessionCompleted) ||
		("sessionAborted" in event && event.sessionAborted) ||
		("sessionResumed" in event && event.sessionResumed) ||
		("sessionTurnStarted" in event && event.sessionTurnStarted) ||
		("sessionTurnDelta" in event && event.sessionTurnDelta) ||
		("sessionTurnReasoningStarted" in event && event.sessionTurnReasoningStarted) ||
		("sessionTurnReasoningDelta" in event && event.sessionTurnReasoningDelta) ||
		("sessionTurnReasoningCompleted" in event &&
			event.sessionTurnReasoningCompleted) ||
		("sessionTurnToolCallStarted" in event && event.sessionTurnToolCallStarted) ||
		("sessionTurnToolCallDelta" in event && event.sessionTurnToolCallDelta) ||
		("sessionTurnToolCallCompleted" in event &&
			event.sessionTurnToolCallCompleted) ||
		("sessionTurnToolCallFailed" in event && event.sessionTurnToolCallFailed) ||
		("sessionTurnCompleted" in event && event.sessionTurnCompleted) ||
		("sessionTurnAborted" in event && event.sessionTurnAborted) ||
		("sessionCheckpointCreated" in event && event.sessionCheckpointCreated) ||
		null;
	return payload ? payload.session_id : null;
}

function eventLabel(event: CoreEvent): string {
	if ("sessionStarted" in event) return "session.started";
	if ("sessionCompleted" in event) return "session.completed";
	if ("sessionAborted" in event) return "session.aborted";
	if ("sessionResumed" in event) return "session.resumed";
	if ("sessionTurnStarted" in event) return "session.turn.started";
	if ("sessionTurnDelta" in event) return "session.turn.delta";
	if ("sessionTurnReasoningStarted" in event) return "session.turn.reasoning.started";
	if ("sessionTurnReasoningDelta" in event) return "session.turn.reasoning.delta";
	if ("sessionTurnReasoningCompleted" in event) return "session.turn.reasoning.completed";
	if ("sessionTurnToolCallStarted" in event) return "session.turn.tool-call.started";
	if ("sessionTurnToolCallDelta" in event) return "session.turn.tool-call.delta";
	if ("sessionTurnToolCallCompleted" in event) return "session.turn.tool-call.completed";
	if ("sessionTurnToolCallFailed" in event) return "session.turn.tool-call.failed";
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
	const sessionTurnReasoningStarted =
		"sessionTurnReasoningStarted" in event ? event.sessionTurnReasoningStarted : null;
	if (sessionTurnReasoningStarted) {
		return `${sessionTurnReasoningStarted.session_id} · ${sessionTurnReasoningStarted.label ?? sessionTurnReasoningStarted.reasoning_id}`;
	}
	const sessionTurnReasoningDelta =
		"sessionTurnReasoningDelta" in event ? event.sessionTurnReasoningDelta : null;
	if (sessionTurnReasoningDelta) {
		const preview =
			sessionTurnReasoningDelta.content.length > 60
				? `${sessionTurnReasoningDelta.content.slice(0, 60)}…`
				: sessionTurnReasoningDelta.content;
		return `${sessionTurnReasoningDelta.session_id} · ${preview}`;
	}
	const sessionTurnToolCallStarted =
		"sessionTurnToolCallStarted" in event ? event.sessionTurnToolCallStarted : null;
	if (sessionTurnToolCallStarted) {
		return `${sessionTurnToolCallStarted.session_id} · ${sessionTurnToolCallStarted.action}`;
	}
	const sessionTurnToolCallDelta =
		"sessionTurnToolCallDelta" in event ? event.sessionTurnToolCallDelta : null;
	if (sessionTurnToolCallDelta) {
		const preview =
			sessionTurnToolCallDelta.content.length > 60
				? `${sessionTurnToolCallDelta.content.slice(0, 60)}…`
				: sessionTurnToolCallDelta.content;
		return `${sessionTurnToolCallDelta.session_id} · ${preview}`;
	}
	const sessionTurnToolCallFailed =
		"sessionTurnToolCallFailed" in event ? event.sessionTurnToolCallFailed : null;
	if (sessionTurnToolCallFailed) {
		return `${sessionTurnToolCallFailed.session_id} · ${sessionTurnToolCallFailed.reason ?? "tool call failed"}`;
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

type FeedScope = "current" | "all";

export function SessionEventFeed({
	events,
	compact = false,
	currentSessionId = null,
}: {
	events: CoreEvent[];
	compact?: boolean;
	/**
	 * When provided, the feed offers a "This session / All" filter so it is
	 * clear the underlying stream is cross-session. Defaults to "This session".
	 */
	currentSessionId?: string | null;
}) {
	const { t } = useTranslation("common");
	const scrollRef = useRef<HTMLDivElement | null>(null);
	const [showScrollToLatest, setShowScrollToLatest] = useState(false);
	const [scope, setScope] = useState<FeedScope>("current");

	const canFilterBySession = Boolean(currentSessionId);
	const effectiveScope: FeedScope = canFilterBySession ? scope : "all";
	const visibleEvents = useMemo(() => {
		if (effectiveScope === "all" || !currentSessionId) {
			return events;
		}
		return events.filter((event) => {
			const id = eventSessionId(event);
			return id === null || id === currentSessionId;
		});
	}, [events, effectiveScope, currentSessionId]);

	const latestEventKey = useMemo(() => {
		const last = visibleEvents[visibleEvents.length - 1];
		return last ? eventLabel(last) : "empty";
	}, [visibleEvents]);

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

	const emptyMessage =
		effectiveScope === "current" && events.length > 0
			? t("sessionEventFeed.emptyForSession")
			: t("sessionEventFeed.emptyTimeline");

	const timelineRows =
		visibleEvents.length === 0 ? (
			<div className="flex min-h-full flex-1 items-center justify-center px-8">
				<p className="m-0 max-w-md text-center text-[13px] leading-relaxed text-muted-foreground">
					{emptyMessage}
				</p>
			</div>
		) : (
			<ul className="m-0 list-none p-0">
				{visibleEvents.map((event, index) => {
					const sessionId = eventSessionId(event);
					const isCurrentSession =
						sessionId !== null && sessionId === currentSessionId;
					return (
						<li
							key={`${eventLabel(event)}-${index}`}
							className="flow-root px-5 pb-1.5 [content-visibility:auto]"
						>
							<div className="dcc-runtime-feed__row dcc-session-event" data-tone={eventTone(event)}>
								<div className="dcc-session-event__header flex flex-wrap items-center gap-1.5">
									<Badge variant={eventTone(event)} className="font-normal">
										{eventLabel(event)}
									</Badge>
									{sessionId ? (
										<Badge
											variant={isCurrentSession ? "secondary" : "outline"}
											className="font-normal"
										>
											{isCurrentSession
												? t("sessionEventFeed.thisSessionBadge")
												: sessionId.slice(0, 8)}
										</Badge>
									) : null}
								</div>
								<p className="dcc-session-event__copy text-[13px] leading-snug">
									{eventPayloadSummary(event)}
								</p>
							</div>
						</li>
					);
				})}
			</ul>
		);

	const scopeToggle = canFilterBySession ? (
		<ToggleGroup
			type="single"
			value={effectiveScope}
			onValueChange={(value) => {
				if (value === "current" || value === "all") {
					setScope(value);
				}
			}}
			className="gap-0.5 rounded-lg border border-border/50 bg-muted/20 p-0.5"
		>
			<ToggleGroupItem value="current" className="h-6 px-2 text-[11px]">
				{t("sessionEventFeed.scopeCurrent")}
			</ToggleGroupItem>
			<ToggleGroupItem value="all" className="h-6 px-2 text-[11px]">
				{t("sessionEventFeed.scopeAll")}
			</ToggleGroupItem>
		</ToggleGroup>
	) : null;

	if (compact) {
		return (
			<div className="flex min-h-0 flex-1 flex-col overflow-hidden">
				{scopeToggle ? (
					<div className="flex shrink-0 items-center justify-end px-3 pb-1 pt-2">
						{scopeToggle}
					</div>
				) : null}
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
							{t("conversation.scrollToBottom")}
						</button>
					</div>
				) : null}
				</div>
			</div>
		);
	}

	return (
		<Card className="dcc-runtime-feed">
			<CardHeader>
				<div className="dcc-card__meta-row">
					<CardTitle>Session events</CardTitle>
					<div className="flex items-center gap-2">
						{scopeToggle}
						<Badge variant="outline">{visibleEvents.length}</Badge>
					</div>
				</div>
			</CardHeader>
			<CardContent className="dcc-runtime-feed__content">{timelineRows}</CardContent>
		</Card>
	);
}
