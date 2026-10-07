import { memo, type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";
import { AlertCircle, Brain, ChevronRight, PauseCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { ToolCallRow } from "@/components/ai/tool-call";
import { Reasoning } from "@/components/ai/reasoning";
import type { AssistantActivityAnnotation } from "./assistant-activity-disclosure";
import { AssistantProse } from "./AssistantProse";
import {
	formatElapsed,
	segmentTurn,
	summarizeTurnWork,
	SUMMARY_KIND_ORDER,
	turnDurationMs,
	type TurnSegment,
} from "./turn-activity.logic";
import "./assistant-activity.css";

type WorkItem = Extract<TurnSegment, { type: "work" }>["items"][number];

/** Long runs show their most recent steps; older ones are one click away. */
const RUN_PAGE_SIZE = 30;

/**
 * Ticks without React commits: the clock writes its own text node, so a
 * running turn does not re-render the transcript every second.
 */
function ElapsedClock({ since }: { since: number }) {
	const ref = useRef<HTMLSpanElement>(null);
	useEffect(() => {
		const update = () => {
			if (ref.current) ref.current.textContent = formatElapsed(Date.now() - since);
		};
		update();
		const timer = window.setInterval(update, 1000);
		return () => window.clearInterval(timer);
	}, [since]);
	return <span ref={ref} className="dcc-turn-clock" />;
}

function WorkItemRow({ item, live }: { item: WorkItem; live: boolean }) {
	const { t } = useTranslation("common");
	if (item.type === "tool-call") return <ToolCallRow annotation={item} live={live} />;
	const thinking = live && Boolean(item.streaming);
	return (
		<div className="dcc-turn-thought">
			<Reasoning
				label={
					thinking
						? t("conversation.activity.turn.thinking")
						: item.label && item.label !== "Thinking"
							? item.label
							: t("conversation.activity.turn.thought")
				}
				defaultOpen={false}
			>
				<div className="whitespace-pre-wrap break-words">
					{item.content.trim() || t("conversation.reasoning.empty")}
				</div>
			</Reasoning>
		</div>
	);
}

function WorkRunList({ items, live }: { items: WorkItem[]; live: boolean }) {
	const { t } = useTranslation("common");
	const [limit, setLimit] = useState(RUN_PAGE_SIZE);
	const hidden = Math.max(0, items.length - limit);
	const visible = hidden > 0 ? items.slice(-limit) : items;
	return (
		<div className="dcc-turn-run-list">
			{hidden > 0 ? (
				<button
					type="button"
					className="dcc-turn-earlier"
					onClick={() => setLimit((value) => value + RUN_PAGE_SIZE)}
				>
					{t("conversation.activity.timeline.earlier", {
						count: Math.min(RUN_PAGE_SIZE, hidden),
					})}
				</button>
			) : null}
			{visible.map((item) => (
				<WorkItemRow key={`${item.type}:${item.id}`} item={item} live={live} />
			))}
		</div>
	);
}

/**
 * The step a live run shows. A thought that already ended says little
 * ("Thought"); the last action the agent took is the informative headline.
 * A thought still in progress stays on top ("Thinking").
 */
export function liveRunHeadline(items: readonly WorkItem[]): WorkItem | undefined {
	const last = items.at(-1);
	if (last?.type === "reasoning" && !last.streaming) {
		for (let index = items.length - 1; index >= 0; index -= 1) {
			const item = items[index];
			if (item?.type === "tool-call") return item;
		}
	}
	return last;
}

/** A burst of work while the turn runs: one line wearing the latest step. */
function LiveWorkRun({ items, live, isLast }: { items: WorkItem[]; live: boolean; isLast: boolean }) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);
	const latest = liveRunHeadline(items);
	if (!latest) return null;
	const active = live && isLast;
	if (open) {
		return (
			<div className="dcc-turn-run" data-open="true">
				<button
					type="button"
					className="dcc-turn-run-toggle"
					aria-expanded
					onClick={() => setOpen(false)}
				>
					<ChevronRight className="size-3 rotate-90" aria-hidden />
					{t("conversation.activity.turn.steps", { count: items.length })}
				</button>
				<WorkRunList items={items} live={active} />
			</div>
		);
	}
	return (
		<div className="dcc-turn-run" data-open="false">
			<div className="dcc-turn-run-current">
				<WorkItemRow item={latest} live={active} />
			</div>
			{items.length > 1 ? (
				<button
					type="button"
					className="dcc-turn-run-more"
					aria-expanded={false}
					onClick={() => setOpen(true)}
				>
					{t("conversation.activity.turn.moreSteps", { count: items.length - 1 })}
				</button>
			) : null}
		</div>
	);
}

function useTurnSummary(annotations: readonly AssistantActivityAnnotation[]) {
	const { t } = useTranslation("common");
	return useMemo(() => {
		const summary = summarizeTurnWork(annotations);
		const parts: string[] = [];
		for (const kind of SUMMARY_KIND_ORDER) {
			const count = kind === "edit" ? summary.editedFiles : (summary.counts[kind] ?? 0);
			if (count > 0) parts.push(t(`conversation.activity.turn.parts.${kind}`, { count }));
		}
		if (parts.length === 0 && summary.thoughts > 0) {
			parts.push(t("conversation.activity.turn.parts.thoughts", { count: summary.thoughts }));
		}
		return { text: parts.join(", "), failures: summary.failures };
	}, [annotations, t]);
}

/**
 * The status line names the phase only; the current step itself is already
 * on the live work row right above it.
 */
function liveStatusLabel(
	segments: TurnSegment[],
	t: ReturnType<typeof useTranslation>["t"],
): string {
	const last = segments.at(-1);
	if (!last || last.type === "prose") return t("conversation.activity.turn.writing");
	return t("conversation.activity.turn.working");
}

/**
 * Everything the agent did in a turn besides its final answer.
 *
 * While the turn runs, narration is shown as it streams and each burst of work
 * is a single line showing the current step. Once it settles, all of it folds
 * behind "Worked for 1m 12s · edited 2 files, ran 3 commands".
 */
export const AssistantActivity = memo(function AssistantActivity({
	annotations,
	turnStreaming,
	interrupted = false,
	waitingForInput = false,
	startedAt,
	endedAt,
	liveMascot,
}: {
	annotations: AssistantActivityAnnotation[];
	turnStreaming?: boolean;
	interrupted?: boolean;
	waitingForInput?: boolean;
	startedAt?: string;
	endedAt?: string;
	/** The agent's robot, shown on the live status line while it works. */
	liveMascot?: ReactNode;
}) {
	const { t } = useTranslation("common");
	const live =
		!interrupted && (turnStreaming ?? annotations.some((item) => item.streaming));
	const segments = useMemo(() => segmentTurn(annotations), [annotations]);
	const summary = useTurnSummary(annotations);
	const [open, setOpen] = useState(false);
	const contentId = useId();
	const mountedAtRef = useRef(Date.now());
	const liveSince = useMemo(() => {
		const parsed = startedAt ? Date.parse(startedAt) : Number.NaN;
		if (Number.isFinite(parsed)) return parsed;
		const first = annotations.find((item) => item.createdAt)?.createdAt;
		const firstParsed = first ? Date.parse(first) : Number.NaN;
		return Number.isFinite(firstParsed) ? firstParsed : mountedAtRef.current;
	}, [annotations, startedAt]);

	if (live) {
		return (
			<div className="dcc-turn dcc-turn-live" data-waiting={waitingForInput ? "true" : "false"}>
				{segments.map((segment, index) =>
					segment.type === "prose" ? (
						<AssistantProse
							key={segment.key}
							content={segment.annotation.content}
							streaming={Boolean(segment.annotation.streaming) && index === segments.length - 1}
							className="dcc-turn-prose"
						/>
					) : (
						<LiveWorkRun
							key={segment.key}
							items={segment.items}
							live={live}
							isLast={index === segments.length - 1}
						/>
					),
				)}
				{/* Sticks to the bottom of the reading area, so the robot and the
				    clock stay in sight while the reply grows past the fold. */}
				<div className="dcc-turn-status" role="status">
					{liveMascot ??
						(waitingForInput ? (
							<PauseCircle className="size-3.5 shrink-0" aria-hidden />
						) : (
							<DccThinkingIndicator size={13} />
						))}
					<span className="dcc-turn-status-label">
						{waitingForInput
							? t("conversation.activity.timeline.waiting")
							: liveStatusLabel(segments, t)}
					</span>
					<ElapsedClock since={liveSince} />
				</div>
			</div>
		);
	}

	const duration = turnDurationMs(startedAt, endedAt);
	const title = interrupted
		? duration != null
			? t("conversation.activity.turn.interrupted", { duration: formatElapsed(duration) })
			: t("conversation.activity.turn.interruptedNoDuration")
		: duration != null
			? t("conversation.activity.turn.worked", { duration: formatElapsed(duration) })
			: t("conversation.activity.turn.workedNoDuration");

	return (
		<div className="dcc-turn dcc-turn-settled" data-open={open ? "true" : "false"}>
			<button
				type="button"
				className="dcc-turn-fold"
				aria-expanded={open}
				aria-controls={contentId}
				onClick={() => setOpen((value) => !value)}
			>
				<ChevronRight className="dcc-turn-fold-chevron size-3.5 shrink-0" aria-hidden />
				{interrupted ? (
					<AlertCircle className="size-3.5 shrink-0" aria-hidden />
				) : summary.text ? null : (
					<Brain className="size-3.5 shrink-0" aria-hidden />
				)}
				<span className="dcc-turn-fold-title">{title}</span>
				{summary.text ? <span className="dcc-turn-fold-summary">· {summary.text}</span> : null}
				{summary.failures > 0 ? (
					<span className="dcc-turn-fold-failures">
						<AlertCircle className="size-3" aria-hidden />
						{t("conversation.activity.failed", { count: summary.failures })}
					</span>
				) : null}
			</button>
			{open ? (
				<div id={contentId} className="dcc-turn-body">
					{segments.map((segment) =>
						segment.type === "prose" ? (
							<AssistantProse
								key={segment.key}
								content={segment.annotation.content}
								className="dcc-turn-prose dcc-turn-prose-folded"
							/>
						) : (
							<WorkRunList key={segment.key} items={segment.items} live={false} />
						),
					)}
				</div>
			) : null}
		</div>
	);
});
