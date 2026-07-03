import { Suspense, useEffect, useMemo, useRef, useState } from "react";
import { Activity, AlertCircle, ChevronRight, Copy, FilePen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { Button } from "@/components/ui/button";
import { LazyStreamdown } from "@/components/streamdown-loader";
import { cn } from "@/lib/utils";
import { Reasoning } from "@/components/ai/reasoning";
import { ToolCall } from "@/components/ai/tool-call";
import { MessageTimestamp } from "./message-metadata";
import { PlanReviewCard } from "./PlanReviewCard";
import { MissionValidationCard } from "./MissionValidationCard";
import { ApprovalCard } from "./ApprovalCard";
import type { AgentInitiatedDelegationRequest } from "@/features/sessions/agent-delegation-request";
import { UserInputCard } from "./UserInputCard";
import type { ParsedPlanContent } from "@/features/panel/plan-content";
import { parseMissionValidationReport } from "@/features/spec/mission-spec-content";
import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";

type AssistantStatus = {
	type: "incomplete";
	reason?: string;
};

function AssistantTextFallback({ text }: { text: string }) {
	return (
		<div className="assistant-markdown-scale max-w-none break-words text-foreground">
			<p className="whitespace-pre-wrap text-[13px] leading-7 text-foreground">
				{text}
			</p>
		</div>
	);
}

function isActivityAnnotation(annotation: WorkspaceMessageAnnotation) {
	return annotation.type === "reasoning" || annotation.type === "tool-call";
}

function AssistantActivityGroup({
	annotations,
	children,
}: {
	annotations: WorkspaceMessageAnnotation[];
	children: React.ReactNode;
}) {
	const { t } = useTranslation("common");
	const isLive = annotations.some((annotation) => Boolean(annotation.streaming));
	const toolCount = annotations.filter((annotation) => annotation.type === "tool-call").length;
	const reasoningCount = annotations.filter((annotation) => annotation.type === "reasoning").length;
	const failedCount = annotations.filter(
		(annotation) => annotation.type === "tool-call" && annotation.status?.type === "failed",
	).length;
	const shouldStayOpen = isLive || failedCount > 0;
	const [isOpen, setIsOpen] = useState(shouldStayOpen);
	// Once the user toggles by hand, auto open/close stops driving this disclosure.
	const userToggledRef = useRef(false);

	useEffect(() => {
		if (userToggledRef.current) {
			return;
		}

		setIsOpen(shouldStayOpen);
	}, [shouldStayOpen]);

	return (
		<details
			className="mb-2 flex min-w-0 flex-col rounded-lg border border-border/50 bg-muted/15 px-2.5 py-2"
			open={isOpen}
		>
			<summary
				onClick={(event) => {
					event.preventDefault();
					userToggledRef.current = true;
					setIsOpen((open) => !open);
				}}
				className="flex cursor-pointer items-center gap-2 text-[12px] text-muted-foreground [&::-webkit-details-marker]:hidden"
			>
				<ChevronRight
					className={cn("size-3 shrink-0 transition-transform", isOpen && "rotate-90")}
					aria-hidden
				/>
				{isLive ? (
					<DccThinkingIndicator size={13} />
				) : (
					<Activity className="size-3.5 shrink-0" aria-hidden />
				)}
				<span className="font-medium text-foreground/85">
					{isLive
						? t("conversation.activity.running")
						: t("conversation.activity.completed")}
				</span>
				<span className="truncate text-muted-foreground/70">
					{t("conversation.activity.summary", {
						actions: toolCount,
						thoughts: reasoningCount,
					})}
				</span>
				{failedCount > 0 ? (
					<span className="ml-auto shrink-0 rounded bg-destructive/10 px-1.5 py-0.5 text-[11px] text-destructive">
						{t("conversation.activity.failed", { count: failedCount })}
					</span>
				) : null}
			</summary>
			<div className="mt-2 flex min-w-0 flex-col gap-1.5 pl-1">
				{children}
			</div>
		</details>
	);
}

export function AssistantMessage({
	content,
	streaming,
	createdAt,
	status,
	annotations,
	plan,
	workspacePath,
	isPlanContext,
	sessionId,
	activeMissionSpecRelativePath,
	activeMissionSpecHash,
	autoSaveMissionValidation,
	onReviewChanges,
	onDelegateTaskApprove,
}: {
	content: string;
	streaming?: boolean;
	createdAt?: string;
	status?: AssistantStatus;
	annotations?: WorkspaceMessageAnnotation[];
	plan?: ParsedPlanContent | null;
	workspacePath?: string | null;
	isPlanContext?: boolean;
	sessionId?: string | null;
	activeMissionSpecRelativePath?: string | null;
	activeMissionSpecHash?: string | null;
	autoSaveMissionValidation?: boolean;
	/** Reveals the inspector to review this turn's edits ([Revisar]). */
	onReviewChanges?: () => void;
	onDelegateTaskApprove?: (request: AgentInitiatedDelegationRequest) => Promise<void>;
}) {
	const { t } = useTranslation("common");
	const showPlanCard = Boolean(isPlanContext || plan?.isPlanLike);
	const activityAnnotations = useMemo(
		() => (annotations ?? []).filter(isActivityAnnotation),
		[annotations],
	);
	const requestAnnotations = useMemo(
		() => (annotations ?? []).filter((annotation) => !isActivityAnnotation(annotation)),
		[annotations],
	);
	// Files this turn edited (deduped) — drives the closing "edits" card. Reads
	// and shell calls are excluded; failed edits are skipped.
	const editedFiles = useMemo(() => {
		const seen = new Map<string, { path: string; name: string }>();
		for (const annotation of annotations ?? []) {
			if (annotation.type !== "tool-call" || !annotation.file) {
				continue;
			}
			if (annotation.status?.type === "failed") {
				continue;
			}
			const action = annotation.action ?? "";
			if (/read/i.test(action)) {
				continue;
			}
			if (!/write|edit|create|update|apply|patch/i.test(action)) {
				continue;
			}
			if (!seen.has(annotation.file)) {
				const name = annotation.file.split("/").pop() || annotation.file;
				seen.set(annotation.file, { path: annotation.file, name });
			}
		}
		return [...seen.values()];
	}, [annotations]);
	const validationReport = showPlanCard ? null : parseMissionValidationReport(content);
	const isValidationStale = Boolean(
		validationReport?.specHash &&
			activeMissionSpecHash &&
			validationReport.specHash !== activeMissionSpecHash,
	);
	return (
		<div
			data-message-role="assistant"
			className="conversation-thread-enter conversation-fade-in group/assistant flex min-w-0 justify-start"
		>
			<div className="relative flex min-w-0 max-w-[75%] flex-col pb-5">
				{activityAnnotations.length ? (
					<AssistantActivityGroup annotations={activityAnnotations}>
						{activityAnnotations.map((annotation) => {
							if (annotation.type === "reasoning") {
								return (
									<Reasoning
										key={`reasoning-${annotation.id}`}
										label={annotation.label ?? t("conversation.reasoning.label")}
										defaultOpen={Boolean(annotation.streaming)}
									>
										<div className="flex items-center gap-1.5">
											{annotation.content.trim().length > 0 ? (
												<span className="whitespace-pre-wrap">{annotation.content}</span>
											) : annotation.streaming ? (
												<>
													<DccThinkingIndicator size={12} />
													<span>{t("conversation.reasoning.label")}</span>
												</>
											) : (
												<span className="text-muted-foreground/70">
													{t("conversation.reasoning.empty")}
												</span>
											)}
										</div>
									</Reasoning>
								);
							}

							return (
								<ToolCall
									key={`tool-call-${annotation.id}`}
									action={annotation.action}
									command={annotation.command}
									file={annotation.file}
									isLive={Boolean(annotation.streaming)}
									isError={annotation.status?.type === "failed"}
								>
									<div className="min-w-0 whitespace-pre-wrap break-words font-mono text-[11px] leading-5">
										{annotation.content.trim().length > 0 ? (
											annotation.content.trimEnd()
										) : annotation.streaming ? (
											<span className="flex items-center gap-1.5 font-sans text-[12px]">
												<DccThinkingIndicator size={12} />
												<span>{t("conversation.toolCall.running")}</span>
											</span>
										) : annotation.status?.type === "failed" ? (
											<span className="font-sans text-[12px]">
												{annotation.status.reason ?? t("conversation.toolCall.failedFallback")}
											</span>
										) : (
											<span className="font-sans text-[12px] text-muted-foreground/70">
												{t("conversation.toolCall.noOutput")}
											</span>
										)}
									</div>
								</ToolCall>
							);
						})}
					</AssistantActivityGroup>
				) : null}
				{requestAnnotations.length ? (
					<div className="mb-2 flex flex-col gap-2">
						{requestAnnotations.map((annotation) => {
							if (annotation.type === "user-input") {
								return (
									<UserInputCard
										key={`user-input-${annotation.id}`}
										sessionId={sessionId ?? null}
										requestId={annotation.id}
										questions={annotation.questions}
										answers={annotation.answers}
										isLive={Boolean(annotation.streaming)}
									/>
								);
							}

							if (annotation.type === "approval") {
								return (
									<ApprovalCard
										key={`approval-${annotation.id}`}
										sessionId={sessionId ?? null}
										requestId={annotation.id}
										toolName={annotation.toolName}
										title={annotation.title}
										description={annotation.description}
										command={annotation.command}
										file={annotation.file}
										behavior={annotation.behavior}
										isLive={Boolean(annotation.streaming)}
										onDelegateTaskApprove={onDelegateTaskApprove}
									/>
								);
							}

							return null;
						})}
					</div>
				) : null}
				{showPlanCard ? (
					<PlanReviewCard plan={plan ?? { title: "Plan", summary: content, steps: [], approvedPrompts: [], rawMarkdown: content, markdown: content, isPlanLike: false, canCollapse: content.length > 900, source: "plain" }} workspacePath={workspacePath} />
				) : validationReport ? (
					<MissionValidationCard
						report={validationReport}
						workspacePath={workspacePath}
						isStale={isValidationStale}
						autoSave={autoSaveMissionValidation}
						activeSpecRelativePath={activeMissionSpecRelativePath}
						activeSpecHash={activeMissionSpecHash}
					/>
				) : (
					<div className={cn("assistant-markdown-scale max-w-none break-words text-foreground")}>
						<Suspense fallback={<AssistantTextFallback text={content} />}>
							<LazyStreamdown
								mode={streaming ? "streaming" : "static"}
								animated={
									streaming
										? { animation: "blurIn", duration: 150, stagger: 30, sep: "word" }
										: false
								}
								caret={streaming ? "block" : undefined}
								className="conversation-streamdown"
								isAnimating={Boolean(streaming)}
								shikiTheme={["github-light", "github-dark"]}
							>
								{content}
							</LazyStreamdown>
						</Suspense>
					</div>
				)}
				{!streaming && !showPlanCard && editedFiles.length > 0 ? (
					<div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-lg border border-border/60 bg-muted/20 px-3 py-2">
						<span className="flex shrink-0 items-center gap-1.5 text-[12px] font-medium text-foreground">
							<FilePen
								className="size-3.5 text-muted-foreground"
								strokeWidth={1.8}
							/>
							{editedFiles.length === 1
								? t("conversation.editsCard.summaryOne")
								: t("conversation.editsCard.summaryOther", {
										count: editedFiles.length,
									})}
						</span>
						<span className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-0.5">
							{editedFiles.map((editedFile) => (
								<span
									key={editedFile.path}
									title={editedFile.path}
									className="truncate font-mono text-[11px] text-muted-foreground"
								>
									{editedFile.name}
								</span>
							))}
						</span>
						{onReviewChanges ? (
							<Button
								type="button"
								variant="outline"
								size="xs"
								className="ml-auto shrink-0"
								onClick={onReviewChanges}
							>
								{t("conversation.editsCard.review")}
							</Button>
						) : null}
					</div>
				) : null}
				<div className="mt-1 flex items-center gap-1.5 text-[11px] leading-none text-muted-foreground/60">
					<MessageTimestamp createdAt={createdAt} />
					{status?.type === "incomplete" ? (
						<span className="inline-flex items-center gap-1 rounded bg-destructive/10 px-1.5 py-0.5 text-destructive">
							<AlertCircle className="size-3" aria-hidden />
							<span>{status.reason ?? "Incomplete"}</span>
						</span>
					) : null}
				</div>
				{showPlanCard ? null : (
					<div className="pointer-events-none absolute right-1 bottom-0 flex items-center justify-end opacity-0 transition-opacity group-hover/assistant:pointer-events-auto group-hover/assistant:opacity-100 group-focus-within/assistant:pointer-events-auto group-focus-within/assistant:opacity-100">
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							aria-label="Copy assistant message"
							className={cn(
								"pointer-events-auto size-5 shrink-0 text-muted-foreground/28 hover:text-muted-foreground",
								"bg-transparent hover:bg-transparent",
							)}
							onClick={async () => {
								try {
									await navigator.clipboard.writeText(content);
								} catch {
									// Clipboard availability varies across desktop shells; ignore gracefully.
								}
							}}
						>
							<Copy className="size-3.5" aria-hidden />
						</Button>
					</div>
				)}
			</div>
		</div>
	);
}
