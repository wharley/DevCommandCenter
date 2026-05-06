import { Suspense } from "react";
import { AlertCircle, Copy } from "lucide-react";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { Button } from "@/components/ui/button";
import { LazyStreamdown } from "@/components/streamdown-loader";
import { cn } from "@/lib/utils";
import { Reasoning } from "@/components/ai/reasoning";
import { ToolCall } from "@/components/ai/tool-call";
import { MessageTimestamp } from "./message-metadata";
import { PlanReviewCard } from "./PlanReviewCard";
import { ApprovalCard } from "./ApprovalCard";
import { UserInputCard } from "./UserInputCard";
import type { ParsedPlanContent } from "@/features/panel/plan-content";
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
}) {
	const showPlanCard = Boolean(isPlanContext || plan?.isPlanLike);
	return (
		<div
			data-message-role="assistant"
			className="conversation-thread-enter conversation-fade-in group/assistant flex min-w-0 justify-start"
		>
			<div className="relative flex min-w-0 max-w-[75%] flex-col pb-5">
				{annotations?.length ? (
					<div className="mb-2 flex flex-col gap-2">
						{annotations.map((annotation) => {
							if (annotation.type === "reasoning") {
								return (
									<Reasoning
										key={`reasoning-${annotation.id}`}
										label={annotation.label ?? "Thinking"}
										defaultOpen={Boolean(annotation.streaming)}
									>
										<div className="flex items-center gap-1.5">
											{annotation.content.trim().length > 0 ? (
												<span className="whitespace-pre-wrap">{annotation.content}</span>
											) : annotation.streaming ? (
												<>
													<DccThinkingIndicator size={12} />
													<span>Thinking</span>
												</>
											) : (
												<span className="text-muted-foreground/70">No reasoning captured.</span>
											)}
										</div>
									</Reasoning>
								);
							}

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
									/>
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
									<div className="flex items-center gap-1.5">
										{annotation.content.trim().length > 0 ? (
											<span className="whitespace-pre-wrap">{annotation.content}</span>
										) : annotation.streaming ? (
											<>
												<DccThinkingIndicator size={12} />
												<span>Running</span>
											</>
										) : annotation.status?.type === "failed" ? (
											<span>{annotation.status.reason ?? "Tool call failed"}</span>
										) : (
											<span className="text-muted-foreground/70">No output captured.</span>
										)}
									</div>
								</ToolCall>
							);
						})}
					</div>
				) : null}
				{showPlanCard ? (
					<PlanReviewCard plan={plan ?? { title: "Plan", summary: content, steps: [], approvedPrompts: [], rawMarkdown: content, markdown: content, isPlanLike: false, canCollapse: content.length > 900, source: "plain" }} workspacePath={workspacePath} />
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
