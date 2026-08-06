import { Suspense, useEffect, useMemo, useRef, useState } from "react";
import { Activity, AlertCircle, ChevronRight, Copy, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { Button } from "@/components/ui/button";
import { LazyStreamdown } from "@/components/streamdown-loader";
import { cn } from "@/lib/utils";
import { Reasoning } from "@/components/ai/reasoning";
import { ToolCall } from "@/components/ai/tool-call";
import { MessageTimestamp } from "./message-metadata";
import { PlanSummaryCard } from "./PlanSummaryCard";
import { MissionValidationCard } from "./MissionValidationCard";
import { ApprovalCard } from "./ApprovalCard";
import type { AgentInitiatedDelegationRequest } from "@/features/sessions/agent-delegation-request";
import { UserInputCard } from "./UserInputCard";
import {
	planNeedsInput,
	type ParsedPlanContent,
} from "@/features/panel/plan-content";
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
	return (
		annotation.type === "commentary" ||
		annotation.type === "reasoning" ||
		annotation.type === "tool-call"
	);
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
	const reasoningCount = annotations.filter(
		(annotation) => annotation.type === "reasoning" || annotation.type === "commentary",
	).length;
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
	isPlanApproved,
	isPlanReadOnly,
	sessionId,
	activeMissionSpecRelativePath,
	activeMissionSpecHash,
	autoSaveMissionValidation,
	onDelegateTaskApprove,
	onContinue,
	onOpenPlan,
	hidePendingApprovals = false,
}: {
	content: string;
	streaming?: boolean;
	createdAt?: string;
	status?: AssistantStatus;
	annotations?: WorkspaceMessageAnnotation[];
	plan?: ParsedPlanContent | null;
	workspacePath?: string | null;
	isPlanContext?: boolean;
	isPlanApproved?: boolean;
	isPlanReadOnly?: boolean;
	sessionId?: string | null;
	activeMissionSpecRelativePath?: string | null;
	activeMissionSpecHash?: string | null;
	autoSaveMissionValidation?: boolean;
	onDelegateTaskApprove?: (request: AgentInitiatedDelegationRequest) => Promise<void>;
	onContinue?: () => void;
	onOpenPlan?: () => void;
	hidePendingApprovals?: boolean;
}) {
	const { t } = useTranslation("common");
	const showPlanCard = Boolean(isPlanContext || plan?.isPlanLike);
	const displayedPlan = plan ?? {
		title: "Plan",
		summary: content,
		steps: [],
		approvedPrompts: [],
		rawMarkdown: content,
		markdown: content,
		isPlanLike: false,
		canCollapse: content.length > 900,
		source: "plain" as const,
	};
	const activityAnnotations = useMemo(
		() => (annotations ?? []).filter(isActivityAnnotation),
		[annotations],
	);
	const requestAnnotations = useMemo(
		() =>
			(annotations ?? []).filter(
				(annotation) =>
					!isActivityAnnotation(annotation) &&
					!(
						hidePendingApprovals &&
						annotation.type === "approval" &&
						annotation.streaming
					),
			),
		[annotations, hidePendingApprovals],
	);
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
			<div
				className={cn(
					"relative flex min-w-0 flex-col pb-5",
					showPlanCard ? "w-full max-w-3xl" : "w-full max-w-[52rem]",
				)}
			>
				{activityAnnotations.length ? (
					<AssistantActivityGroup annotations={activityAnnotations}>
						{activityAnnotations.map((annotation) => {
							if (annotation.type === "commentary") {
								return (
									<div
										key={`commentary-${annotation.id}`}
										className="rounded-md px-2 py-1.5 text-[12px] leading-5 text-muted-foreground"
									>
										<div className="mb-0.5 font-medium text-foreground/75">
											{t("conversation.commentary.label")}
										</div>
										<div className="whitespace-pre-wrap break-words">
											{annotation.content}
										</div>
									</div>
								);
							}
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
					<PlanSummaryCard
						plan={displayedPlan}
						needsInput={planNeedsInput(displayedPlan.rawMarkdown)}
						approved={isPlanApproved}
						readOnly={isPlanReadOnly}
						onOpen={onOpenPlan ?? (() => undefined)}
					/>
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
				<div className="mt-1 flex items-center gap-1.5 text-[11px] leading-none text-muted-foreground/60">
					<MessageTimestamp createdAt={createdAt} />
				</div>
				{status?.type === "incomplete" ? (
					<div className="mt-2 flex max-w-2xl items-start gap-2.5 rounded-lg border border-destructive/20 bg-destructive/[0.045] px-3 py-2.5">
						<AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" aria-hidden />
						<div className="min-w-0 flex-1">
							<div className="text-[12px] font-medium text-foreground">
								{t("conversation.message.interrupted")}
							</div>
							<div className="mt-0.5 line-clamp-2 break-words text-[11px] leading-4 text-muted-foreground">
								{status.reason ?? t("conversation.message.incomplete")}
							</div>
						</div>
						{onContinue ? (
							<Button
								type="button"
								variant="outline"
								size="sm"
								className="h-7 shrink-0 gap-1.5 px-2 text-[11px]"
								onClick={onContinue}
							>
								<RotateCcw className="size-3" aria-hidden />
								{t("conversation.message.continue")}
							</Button>
						) : null}
					</div>
				) : null}
				{showPlanCard ? null : (
					<div className="pointer-events-none absolute right-1 bottom-0 flex items-center justify-end opacity-0 transition-opacity group-hover/assistant:pointer-events-auto group-hover/assistant:opacity-100 group-focus-within/assistant:pointer-events-auto group-focus-within/assistant:opacity-100">
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							aria-label={t("conversation.message.copyAssistant")}
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
