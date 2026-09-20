import { memo, useId, useMemo, useRef, useState } from "react";
import {
	Activity,
	AlertCircle,
	ChevronDown,
	ChevronRight,
	MessageSquare,
	PauseCircle,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { ToolCall } from "@/components/ai/tool-call";
import { Reasoning } from "@/components/ai/reasoning";
import {
	ASSISTANT_ACTIVITY_PAGE_SIZE,
	classifyAssistantActivityAction,
	selectAssistantActivity,
	summarizeAssistantActivity,
	type AssistantActivityAnnotation,
	type ActivityFilter,
} from "./assistant-activity-disclosure";
import "./assistant-activity.css";

export const AssistantActivity = memo(function AssistantActivity({
	annotations,
	turnStreaming,
	interrupted = false,
	waitingForInput = false,
}: {
	annotations: AssistantActivityAnnotation[];
	turnStreaming?: boolean;
	interrupted?: boolean;
	waitingForInput?: boolean;
}) {
	const { t } = useTranslation("common");
	const summary = useMemo(
		() =>
			summarizeAssistantActivity(
				annotations,
				turnStreaming,
				interrupted,
				waitingForInput,
			),
		[annotations, turnStreaming, interrupted, waitingForInput],
	);
	// Keep the live activity compact by default. The latest record remains
	// visible in the summary row, and the user can expand the full timeline.
	const [isOpen, setIsOpen] = useState(false);
	const [filter, setFilter] = useState<ActivityFilter>("all");
	const [limit, setLimit] = useState(ASSISTANT_ACTIVITY_PAGE_SIZE);
	const manualRef = useRef(false);
	const readingBaselineRef = useRef(annotations.length);
	const markManual = () => {
		if (!manualRef.current) readingBaselineRef.current = annotations.length;
		manualRef.current = true;
	};
	const contentId = useId();
	// Retain already visible entries while the user reads and new events arrive.
	const visibleLimit =
		limit +
		(manualRef.current
			? Math.max(0, annotations.length - readingBaselineRef.current)
			: 0);
	const windowed = useMemo(
		() => selectAssistantActivity(annotations, filter, visibleLimit),
		[annotations, filter, visibleLimit],
	);
	const changeFilter = (value: ActivityFilter) => {
		markManual();
		readingBaselineRef.current = annotations.length;
		setFilter(value);
		setLimit(ASSISTANT_ACTIVITY_PAGE_SIZE);
		setIsOpen(true);
	};
	const latest = summary.latest;
	const preview =
		latest?.type === "tool-call"
			? getToolActionLabel(latest.action, t)
			: latest?.content.trim() ||
				(latest?.type === "reasoning"
					? latest.label || t("conversation.reasoning.label")
					: "");
	const counts = {
		all: annotations.length,
		tools: summary.tools,
		updates: summary.updates,
		failures: summary.failures,
	};
	const running = summary.state === "running";
	return (
		<div
			className="dcc-assistant-activity dcc-activity-timeline"
			data-live={summary.live ? "true" : "false"}
			data-state={isOpen ? "open" : "closed"}
			onPointerDown={markManual}
			onFocusCapture={markManual}
		>
			<div className="dcc-activity-heading">
				<span
					className="dcc-activity-mark"
					data-status={summary.state}
					aria-hidden
				>
					{running ? (
						<DccThinkingIndicator size={15} />
					) : summary.state === "waiting" ? (
						<PauseCircle size={16} />
					) : interrupted ? (
						<AlertCircle size={16} />
					) : (
						<Activity size={16} />
					)}
				</span>
				<div className="dcc-activity-heading-copy">
					<span className="dcc-activity-title" role="status">
						{t(`conversation.activity.timeline.${summary.state}`)}
					</span>
					<span className="dcc-activity-total">
						{t("conversation.activity.timeline.records", {
							count: annotations.length,
						})}
					</span>
				</div>
				{summary.failures > 0 && (
					<button
						type="button"
						className="dcc-activity-failures"
						onClick={() => changeFilter("failures")}
						aria-label={t("conversation.activity.timeline.showFailures", {
							count: summary.failures,
						})}
					>
						<AlertCircle size={12} aria-hidden />
						{t("conversation.activity.failed", { count: summary.failures })}
					</button>
				)}
				<button
					type="button"
					className="dcc-activity-toggle"
					aria-expanded={isOpen}
					aria-controls={contentId}
					aria-label={t(
						isOpen
							? "conversation.activity.timeline.collapse"
							: "conversation.activity.timeline.expand",
					)}
					onClick={() => {
						markManual();
						setIsOpen((value) => !value);
					}}
				>
					<ChevronDown
						size={16}
						className={isOpen ? "rotate-180" : ""}
						aria-hidden
					/>
				</button>
			</div>
			{preview && (
				<div className="dcc-activity-latest">
					<span>{t("conversation.activity.timeline.latest")}</span>
					<p>
						{preview.slice(0, 280)}
						{preview.length > 280 ? "…" : ""}
					</p>
				</div>
			)}
			{summary.failures > 0 && (
				<p className="dcc-activity-attention">
					<AlertCircle size={13} aria-hidden />
					{t("conversation.activity.timeline.attention", {
						count: summary.failures,
					})}
				</p>
			)}
			{isOpen && (
				<div id={contentId} className="dcc-activity-body">
					<div
						className="dcc-activity-filters"
						role="group"
						aria-label={t("conversation.activity.timeline.filterLabel")}
					>
						{(["all", "tools", "updates", "failures"] as const).map((value) => (
							<button
								type="button"
								key={value}
								aria-pressed={filter === value}
								onClick={() => changeFilter(value)}
							>
								{t(`conversation.activity.timeline.filters.${value}`)}
								<span>{counts[value]}</span>
							</button>
						))}
					</div>
					{windowed.hidden > 0 && (
						<button
							type="button"
							className="dcc-activity-earlier"
							onClick={() => {
								markManual();
								setLimit((value) => value + ASSISTANT_ACTIVITY_PAGE_SIZE);
							}}
						>
							<ChevronRight size={13} aria-hidden />
							{t("conversation.activity.timeline.earlier", {
								count: Math.min(ASSISTANT_ACTIVITY_PAGE_SIZE, windowed.hidden),
							})}
							<span>
								{t("conversation.activity.timeline.remaining", {
									count: windowed.hidden,
								})}
							</span>
						</button>
					)}
					<ol
						className="dcc-activity-steps"
						aria-label={t("conversation.activity.timeline.steps")}
					>
						{windowed.entries.map(({ annotation, index }) => (
							<li
								key={`${annotation.type}:${annotation.id}`}
								className="dcc-activity-step"
								data-activity-id={annotation.id}
							>
								<span className="dcc-activity-ordinal" aria-hidden>
									{String(index + 1).padStart(2, "0")}
								</span>
								<div className="dcc-activity-step-content">
									<ActivityEntry
										annotation={annotation}
										live={summary.live && !waitingForInput}
									/>
								</div>
							</li>
						))}
					</ol>
					{!windowed.total && (
						<p className="dcc-activity-empty" role="status">
							{t("conversation.activity.timeline.empty")}
						</p>
					)}
				</div>
			)}
		</div>
	);
});

function ActivityEntry({
	annotation,
	live,
}: {
	annotation: AssistantActivityAnnotation;
	live: boolean;
}) {
	const { t } = useTranslation("common");
	if (annotation.type === "commentary")
		return (
			<div className="dcc-activity-commentary">
				<div>
					<MessageSquare size={13} aria-hidden />
					<span>{t("conversation.commentary.label")}</span>
				</div>
				<p>{annotation.content}</p>
			</div>
		);
	if (annotation.type === "reasoning")
		return (
			<Reasoning
				label={annotation.label ?? t("conversation.reasoning.label")}
				defaultOpen={live && Boolean(annotation.streaming)}
			>
				<div className="whitespace-pre-wrap break-words">
					{annotation.content.trim() ||
						t(
							live && annotation.streaming
								? "conversation.reasoning.label"
								: "conversation.reasoning.empty",
						)}
				</div>
			</Reasoning>
		);
	const failed = annotation.status?.type === "failed";
	return (
		<ToolCall
			action={getToolActionLabel(annotation.action, t)}
			command={annotation.command}
			file={annotation.file}
			isLive={!failed && live && Boolean(annotation.streaming)}
			isError={failed}
			isUnfinished={!failed && !live && Boolean(annotation.streaming)}
		>
			<div className="min-w-0 whitespace-pre-wrap break-words font-mono text-[11px] leading-5">
				{annotation.content.trim()
					? annotation.content.trimEnd()
					: failed
						? (annotation.status?.reason ??
							t("conversation.toolCall.failedFallback"))
						: live && annotation.streaming
							? t("conversation.toolCall.running")
							: t("conversation.toolCall.noOutput")}
			</div>
		</ToolCall>
	);
}

function getToolActionLabel(
	action: string,
	t: ReturnType<typeof useTranslation>["t"],
) {
	const kind = classifyAssistantActivityAction(action);
	if (kind) return t(`conversation.activity.timeline.actions.${kind}`);
	return /\s/.test(action.trim())
		? action
		: t("conversation.activity.timeline.actions.generic");
}
