import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { ChevronDown } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { CoreEvent } from "@dcc/contracts";
import { isSemanticSessionEvent } from "./session-event-feed.logic";

/** Session id that an event belongs to, or null for non-session events. */
function eventSessionId(event: CoreEvent): string | null {
	const payload =
		("sessionStarted" in event && event.sessionStarted) ||
		("sessionCompleted" in event && event.sessionCompleted) ||
		("sessionAborted" in event && event.sessionAborted) ||
		("sessionResumed" in event && event.sessionResumed) ||
		("sessionObjectivePaused" in event && event.sessionObjectivePaused) ||
		("sessionMcpRuntimeStatusChanged" in event &&
			event.sessionMcpRuntimeStatusChanged) ||
		("sessionTurnStarted" in event && event.sessionTurnStarted) ||
		("sessionTurnDelta" in event && event.sessionTurnDelta) ||
		("sessionTurnAssistantMessageStarted" in event &&
			event.sessionTurnAssistantMessageStarted) ||
		("sessionTurnAssistantMessageDelta" in event &&
			event.sessionTurnAssistantMessageDelta) ||
		("sessionTurnAssistantMessageCompleted" in event &&
			event.sessionTurnAssistantMessageCompleted) ||
		("sessionTurnReasoningStarted" in event && event.sessionTurnReasoningStarted) ||
		("sessionTurnReasoningDelta" in event && event.sessionTurnReasoningDelta) ||
		("sessionTurnReasoningCompleted" in event &&
			event.sessionTurnReasoningCompleted) ||
		("sessionTurnToolCallStarted" in event && event.sessionTurnToolCallStarted) ||
		("sessionTurnToolCallDelta" in event && event.sessionTurnToolCallDelta) ||
		("sessionTurnToolCallCompleted" in event &&
			event.sessionTurnToolCallCompleted) ||
		("sessionTurnToolCallFailed" in event && event.sessionTurnToolCallFailed) ||
		("sessionTurnNativeSubagentActivity" in event &&
			event.sessionTurnNativeSubagentActivity) ||
		("sessionTurnCompleted" in event && event.sessionTurnCompleted) ||
		("sessionTurnAborted" in event && event.sessionTurnAborted) ||
		("sessionCheckpointCreated" in event && event.sessionCheckpointCreated) ||
		("sessionPlanApproved" in event && event.sessionPlanApproved) ||
		("sessionPlanHandedOff" in event && event.sessionPlanHandedOff) ||
		null;
	return payload ? payload.session_id : null;
}

function eventLabel(event: CoreEvent): string {
	if ("sessionStarted" in event) return "session.started";
	if ("sessionCompleted" in event) return "session.completed";
	if ("sessionAborted" in event) return "session.aborted";
	if ("sessionResumed" in event) return "session.resumed";
	if ("sessionObjectivePaused" in event) return "session.objective.paused";
	if ("sessionMcpRuntimeStatusChanged" in event) return "session.mcp.runtime-status";
	if ("sessionTurnStarted" in event) return "session.turn.started";
	if ("sessionTurnDelta" in event) return "session.turn.delta";
	if ("sessionTurnAssistantMessageStarted" in event)
		return "session.turn.assistant-message.started";
	if ("sessionTurnAssistantMessageDelta" in event)
		return "session.turn.assistant-message.delta";
	if ("sessionTurnAssistantMessageCompleted" in event)
		return "session.turn.assistant-message.completed";
	if ("sessionTurnReasoningStarted" in event) return "session.turn.reasoning.started";
	if ("sessionTurnReasoningDelta" in event) return "session.turn.reasoning.delta";
	if ("sessionTurnReasoningCompleted" in event) return "session.turn.reasoning.completed";
	if ("sessionTurnToolCallStarted" in event) return "session.turn.tool-call.started";
	if ("sessionTurnToolCallDelta" in event) return "session.turn.tool-call.delta";
	if ("sessionTurnToolCallCompleted" in event) return "session.turn.tool-call.completed";
	if ("sessionTurnToolCallFailed" in event) return "session.turn.tool-call.failed";
	if ("sessionTurnNativeSubagentActivity" in event)
		return "session.turn.native-subagent.activity";
	if ("sessionTurnCompleted" in event) return "session.turn.completed";
	if ("sessionTurnAborted" in event) return "session.turn.aborted";
	if ("sessionCheckpointCreated" in event) return "session.checkpoint.created";
	if ("sessionPlanApproved" in event) return "session.plan.approved";
	if ("sessionPlanHandedOff" in event) return "session.plan.handed-off";
	if ("workspacePrepared" in event) return "workspace.prepared";
	if ("workspaceReady" in event) return "workspace.ready";
	return "event";
}

function eventTone(event: CoreEvent): "outline" | "secondary" | "success" | "warn" {
	if (
		"sessionCompleted" in event ||
		"workspaceReady" in event ||
		"sessionPlanApproved" in event ||
		"sessionPlanHandedOff" in event
	) {
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

function semanticEventPresentation(
	event: CoreEvent,
	t: TFunction<"common">,
): { title: string; description: string } {
	if ("workspacePrepared" in event && event.workspacePrepared) {
		return {
			title: t("sessionEventFeed.milestones.workspacePrepared"),
			description: t("sessionEventFeed.details.workspacePrepared", {
				path: event.workspacePrepared.worktree_path,
			}),
		};
	}
	if ("workspaceReady" in event && event.workspaceReady) {
		return {
			title: t("sessionEventFeed.milestones.workspaceReady"),
			description: t("sessionEventFeed.details.workspaceReady", {
				path: event.workspaceReady.worktree_path,
			}),
		};
	}
	if ("sessionStarted" in event && event.sessionStarted) {
		return {
			title: t("sessionEventFeed.milestones.sessionStarted"),
			description: t("sessionEventFeed.details.sessionStarted", {
				provider: event.sessionStarted.provider_id,
			}),
		};
	}
	if ("sessionResumed" in event && event.sessionResumed) {
		return {
			title: t("sessionEventFeed.milestones.sessionResumed"),
			description: t("sessionEventFeed.details.sessionResumed"),
		};
	}
	if ("sessionObjectivePaused" in event && event.sessionObjectivePaused) {
		return {
			title: t("sessionEventFeed.milestones.objectivePaused"),
			description: t(
				`sessionEventFeed.details.objectivePaused.${event.sessionObjectivePaused.reason}`,
				{
					failures: event.sessionObjectivePaused.consecutive_failures,
					turns: event.sessionObjectivePaused.turns_used,
				},
			),
		};
	}
	if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
		return {
			title: t("sessionEventFeed.milestones.turnStarted"),
			description: event.sessionTurnStarted.prompt,
		};
	}
	if ("sessionTurnToolCallFailed" in event && event.sessionTurnToolCallFailed) {
		return {
			title: t("sessionEventFeed.milestones.toolFailed"),
			description:
				event.sessionTurnToolCallFailed.reason ??
				t("sessionEventFeed.details.noFailureReason"),
		};
	}
	if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
		return {
			title: t("sessionEventFeed.milestones.turnCompleted"),
			description: t("sessionEventFeed.details.turnCompleted"),
		};
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return {
			title: t("sessionEventFeed.milestones.turnAborted"),
			description:
				event.sessionTurnAborted.reason ??
				t("sessionEventFeed.details.noAbortReason"),
		};
	}
	if ("sessionCheckpointCreated" in event && event.sessionCheckpointCreated) {
		return {
			title: t("sessionEventFeed.milestones.checkpointCreated"),
			description: event.sessionCheckpointCreated.label,
		};
	}
	if ("sessionPlanApproved" in event && event.sessionPlanApproved) {
		return {
			title: t("sessionEventFeed.milestones.planApproved"),
			description: t("sessionEventFeed.details.planApproved", {
				version: event.sessionPlanApproved.plan_version,
			}),
		};
	}
	if ("sessionPlanHandedOff" in event && event.sessionPlanHandedOff) {
		return {
			title: t("sessionEventFeed.milestones.planHandedOff"),
			description: t("sessionEventFeed.details.planHandedOff", {
				version: event.sessionPlanHandedOff.plan_version,
			}),
		};
	}
	if ("sessionCompleted" in event && event.sessionCompleted) {
		return {
			title: t("sessionEventFeed.milestones.sessionCompleted"),
			description: t("sessionEventFeed.details.sessionCompleted"),
		};
	}
	if ("sessionAborted" in event && event.sessionAborted) {
		return {
			title: t("sessionEventFeed.milestones.sessionAborted"),
			description:
				event.sessionAborted.reason ?? t("sessionEventFeed.details.noAbortReason"),
		};
	}
	return {
		title: t("sessionEventFeed.milestones.event"),
		description: t("sessionEventFeed.details.event"),
	};
}

function eventPayloadSummary(event: CoreEvent, t: TFunction<"common">): string {
	const sessionStarted = "sessionStarted" in event ? event.sessionStarted : null;
	if (sessionStarted) {
		return `${sessionStarted.session_id} · ${sessionStarted.provider_id}`;
	}
	const sessionTurnStarted =
		"sessionTurnStarted" in event ? event.sessionTurnStarted : null;
	if (sessionTurnStarted) {
		return `${sessionTurnStarted.session_id} · ${sessionTurnStarted.prompt}`;
	}
	const mcpRuntimeStatus =
		"sessionMcpRuntimeStatusChanged" in event
			? event.sessionMcpRuntimeStatusChanged
			: null;
	if (mcpRuntimeStatus) {
		const states = mcpRuntimeStatus.statuses
			.map((status) => `${status.definitionId}:${status.state}`)
			.join(", ");
		return `${mcpRuntimeStatus.session_id}${states ? ` · ${states}` : ""}`;
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
		return `${sessionTurnToolCallFailed.session_id} · ${
			sessionTurnToolCallFailed.reason ??
			t("sessionEventFeed.details.noFailureReason")
		}`;
	}
	const sessionTurnAborted =
		"sessionTurnAborted" in event ? event.sessionTurnAborted : null;
	if (sessionTurnAborted) {
		return `${sessionTurnAborted.session_id} · ${
			sessionTurnAborted.reason ?? t("sessionEventFeed.details.noAbortReason")
		}`;
	}
	const sessionCheckpointCreated =
		"sessionCheckpointCreated" in event ? event.sessionCheckpointCreated : null;
	if (sessionCheckpointCreated) {
		return `${sessionCheckpointCreated.session_id} · ${sessionCheckpointCreated.label}`;
	}
	const sessionPlanApproved =
		"sessionPlanApproved" in event ? event.sessionPlanApproved : null;
	if (sessionPlanApproved) {
		return `${sessionPlanApproved.session_id} · v${sessionPlanApproved.plan_version} · ${sessionPlanApproved.plan_hash}`;
	}
	const sessionPlanHandedOff =
		"sessionPlanHandedOff" in event ? event.sessionPlanHandedOff : null;
	if (sessionPlanHandedOff) {
		return `${sessionPlanHandedOff.session_id} · v${sessionPlanHandedOff.plan_version} · ${sessionPlanHandedOff.action}`;
	}
	if ("workspacePrepared" in event || "workspaceReady" in event) {
		const payload = "workspacePrepared" in event ? event.workspacePrepared : event.workspaceReady;
		return payload
			? `${payload.project_id} · ${payload.worktree_path}`
			: t("sessionEventFeed.diagnostics.noPayload");
	}
	const sessionCompleted =
		"sessionCompleted" in event ? event.sessionCompleted : null;
	if (sessionCompleted) {
		return sessionCompleted.session_id;
	}
	const sessionAborted = "sessionAborted" in event ? event.sessionAborted : null;
	if (sessionAborted) {
		return `${sessionAborted.session_id} · ${
			sessionAborted.reason ?? t("sessionEventFeed.details.noAbortReason")
		}`;
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
	return t("sessionEventFeed.diagnostics.noPayload");
}

type FeedScope = "current" | "all";
type FeedView = "summary" | "diagnostics";

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
	const [view, setView] = useState<FeedView>("summary");

	const canFilterBySession = Boolean(currentSessionId);
	const effectiveScope: FeedScope = canFilterBySession ? scope : "all";
	const scopedEvents = useMemo(() => {
		if (effectiveScope === "all" || !currentSessionId) {
			return events;
		}
		return events.filter((event) => {
			const id = eventSessionId(event);
			return id === null || id === currentSessionId;
		});
	}, [events, effectiveScope, currentSessionId]);
	const visibleEvents = useMemo(
		() =>
			view === "summary"
				? scopedEvents.filter(isSemanticSessionEvent)
				: scopedEvents,
		[scopedEvents, view],
	);

	const latestEventKey = useMemo(() => {
		const last = visibleEvents[visibleEvents.length - 1];
		return last ? `${view}-${eventLabel(last)}-${visibleEvents.length}` : `${view}-empty`;
	}, [view, visibleEvents]);

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
		view === "summary" && scopedEvents.length > 0
			? t("sessionEventFeed.emptySummary")
			: effectiveScope === "current" && events.length > 0
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
					const semantic =
						view === "summary" ? semanticEventPresentation(event, t) : null;
					return (
						<li
							key={`${eventLabel(event)}-${index}`}
							className="flow-root px-5 pb-1.5 [content-visibility:auto]"
						>
							<div className="dcc-runtime-feed__row dcc-session-event" data-tone={eventTone(event)}>
								<div className="dcc-session-event__header flex flex-wrap items-center gap-1.5">
									<Badge variant={eventTone(event)} className="font-normal">
										{semantic?.title ?? eventLabel(event)}
									</Badge>
									{view === "diagnostics" && sessionId ? (
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
					{semantic?.description ?? eventPayloadSummary(event, t)}
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
	const viewToggle = (
		<ToggleGroup
			type="single"
			value={view}
			onValueChange={(value) => {
				if (value === "summary" || value === "diagnostics") {
					setView(value);
				}
			}}
			className="gap-0.5 rounded-lg border border-border/50 bg-muted/20 p-0.5"
		>
			<ToggleGroupItem value="summary" className="h-6 px-2 text-[11px]">
				{t("sessionEventFeed.viewSummary")}
			</ToggleGroupItem>
			<ToggleGroupItem value="diagnostics" className="h-6 px-2 text-[11px]">
				{t("sessionEventFeed.viewDiagnostics")}
			</ToggleGroupItem>
		</ToggleGroup>
	);

	if (compact) {
		return (
			<div className="flex min-h-0 flex-1 flex-col overflow-hidden">
				<div className="flex shrink-0 flex-wrap items-center justify-between gap-2 px-3 pb-1 pt-2">
					{viewToggle}
					{scopeToggle}
				</div>
				<div className="dcc-conversation-scroll-area relative min-h-0 flex-1 overflow-hidden">
					<div
						ref={scrollRef}
						data-inspector-scroll-key="activity-feed"
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
					<CardTitle>{t("sessionEventFeed.title")}</CardTitle>
					<div className="flex items-center gap-2">
						{viewToggle}
						{scopeToggle}
						<Badge variant="outline">{visibleEvents.length}</Badge>
					</div>
				</div>
			</CardHeader>
			<CardContent className="dcc-runtime-feed__content">{timelineRows}</CardContent>
		</Card>
	);
}
