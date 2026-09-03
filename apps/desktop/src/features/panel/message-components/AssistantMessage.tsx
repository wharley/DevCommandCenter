import {
	Children,
	Suspense,
	useCallback,
	useEffect,
	useId,
	useMemo,
	useRef,
	useState,
} from "react";
import {
	Activity,
	AlertCircle,
	Bot,
	ChevronRight,
	Copy,
	GitBranch,
	GitFork,
	MessageSquarePlus,
	RotateCcw,
	Square,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { ProviderCatalog } from "@dcc/contracts";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { LazyStreamdown } from "@/components/streamdown-loader";
import { WorkspaceFileLinkProvider } from "@/components/workspace-file-link-context";
import type { WorkspaceFileReference } from "@/components/workspace-file-reference";
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
import {
	ASSISTANT_ACTIVITY_AUTO_COLLAPSE_DELAY_MS,
	partitionAssistantActivity,
	shouldAutoOpenAssistantActivity,
} from "./assistant-activity-disclosure";
import {
	resolveNativeSubagentPresentation,
} from "../native-subagent-presentation";
import {
	nativeSubagentControlAvailability,
	nativeSubagentDisplayStatus,
	projectNativeSubagentTree,
	type NativeSubagentAnnotation,
	type NativeSubagentTreeNode,
} from "../native-subagent-tree";
import {
	interruptNativeSubagent,
	steerNativeSubagent,
} from "@/lib/session-api";
import {
	ASSISTANT_STREAMDOWN_SHIKI_THEME,
	assistantStreamingAnimation,
} from "./assistant-streaming-rendering";

type AssistantStatus = {
	type: "incomplete";
	reason?: string;
};

type NativeSubagentSupervision = {
	sessionId?: string | null;
	parentStreaming?: boolean;
	supportsSteering?: boolean;
	supportsInterrupt?: boolean;
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

function resolveModelLabel(
	model: string | null | undefined,
	providers: ProviderCatalog["providers"] = [],
) {
	const normalized = model?.trim();
	if (!normalized) return null;
	return (
		providers
			.flatMap((provider) => provider.models)
			.find((candidate) => candidate.id === normalized)?.label ?? normalized
	);
}

function NativeSubagentCard({
	annotation,
	providers,
	treeLabel,
	nested = false,
	supervision,
}: {
	annotation: NativeSubagentAnnotation;
	providers?: ProviderCatalog["providers"];
	treeLabel?: string;
	nested?: boolean;
	supervision?: NativeSubagentSupervision;
}) {
	const { t } = useTranslation("common");
	const [instructionOpen, setInstructionOpen] = useState(false);
	const [instruction, setInstruction] = useState("");
	const [pendingAction, setPendingAction] = useState<"steer" | "interrupt" | null>(null);
	const {
		modelName: modelLabel,
		requestedModelName: requestedModelLabel,
		agentName,
		identity,
	} = resolveNativeSubagentPresentation(annotation, providers);
	const details = [
		agentName,
		annotation.role,
		requestedModelLabel && modelLabel && requestedModelLabel !== modelLabel
			? `${t("conversation.requestedModelLabel")}: ${requestedModelLabel}`
			: null,
	]
		.filter(
			(value, index, all) =>
				Boolean(value) && value !== identity && all.indexOf(value) === index,
		)
		.join(" · ");
	const status = t(
		`conversation.nativeSubagent.status.${nativeSubagentDisplayStatus(
			annotation,
			supervision?.parentStreaming,
		)}`,
	);
	const controls = nativeSubagentControlAvailability(annotation, supervision ?? {});
	const canControl = controls.canSteer || controls.canInterrupt;

	const handleSteer = async () => {
		const prompt = instruction.trim();
		if (!prompt || !supervision?.sessionId || !annotation.agentThreadId) return;
		setPendingAction("steer");
		try {
			await steerNativeSubagent({
				sessionId: supervision.sessionId,
				agentThreadId: annotation.agentThreadId,
				prompt,
			});
			setInstruction("");
			setInstructionOpen(false);
			toast.success(t("conversation.nativeSubagent.control.instructionSent"));
		} catch (error) {
			toast.error(t("conversation.nativeSubagent.control.instructionFailed"), {
				description: error instanceof Error ? error.message : String(error),
			});
		} finally {
			setPendingAction(null);
		}
	};

	const handleInterrupt = async () => {
		if (!supervision?.sessionId || !annotation.agentThreadId) return;
		setPendingAction("interrupt");
		try {
			await interruptNativeSubagent({
				sessionId: supervision.sessionId,
				agentThreadId: annotation.agentThreadId,
			});
			setInstructionOpen(false);
			toast.success(t("conversation.nativeSubagent.control.interrupted"));
		} catch (error) {
			toast.error(t("conversation.nativeSubagent.control.interruptFailed"), {
				description: error instanceof Error ? error.message : String(error),
			});
		} finally {
			setPendingAction(null);
		}
	};

	return (
		<div
			className={cn(
				"flex min-w-0 flex-col gap-2 rounded-lg border border-border/50 bg-muted/15 px-2.5 py-2 text-[12px]",
				!nested && "mb-2",
			)}
			title={annotation.agentThreadId ?? undefined}
		>
			<div className="flex min-w-0 flex-wrap items-center gap-2">
				<Bot className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
				<div className="min-w-0 flex-1">
					<div className="flex min-w-0 items-center gap-1.5">
						<span className="shrink-0 font-medium text-foreground/85">
							{treeLabel ?? t("conversation.nativeSubagent.label")}
						</span>
						{identity && identity !== treeLabel ? (
							<span className="truncate text-foreground/85">{identity}</span>
						) : null}
					</div>
					{details ? <div className="truncate text-muted-foreground">{details}</div> : null}
				</div>
				<span className="ml-auto shrink-0 text-[11px] text-muted-foreground">
					{status}
				</span>
				{canControl ? (
					<div className="flex shrink-0 items-center gap-1">
						{controls.canSteer ? (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="h-7 gap-1 px-2 text-[11px]"
								disabled={pendingAction != null}
								onClick={() => setInstructionOpen((open) => !open)}
							>
								<MessageSquarePlus className="size-3" aria-hidden />
								{t("conversation.nativeSubagent.control.instruct")}
							</Button>
						) : null}
						{controls.canInterrupt ? (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="h-7 gap-1 px-2 text-[11px] text-destructive hover:text-destructive"
								disabled={pendingAction != null}
								onClick={() => void handleInterrupt()}
							>
								<Square className="size-3" aria-hidden />
								{t("conversation.nativeSubagent.control.interrupt")}
							</Button>
						) : null}
					</div>
				) : null}
			</div>
			{instructionOpen && controls.canSteer ? (
				<form
					className="ml-5 flex min-w-0 flex-col gap-2"
					onSubmit={(event) => {
						event.preventDefault();
						void handleSteer();
					}}
				>
					<Textarea
						value={instruction}
						onChange={(event) => setInstruction(event.target.value)}
						placeholder={t("conversation.nativeSubagent.control.instructionPlaceholder")}
						maxLength={32_000}
						rows={2}
						className="min-h-16 resize-y text-[12px]"
						autoFocus
					/>
					<div className="flex justify-end gap-1.5">
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="h-7 px-2 text-[11px]"
							disabled={pendingAction != null}
							onClick={() => setInstructionOpen(false)}
						>
							{t("conversation.nativeSubagent.control.cancel")}
						</Button>
						<Button
							type="submit"
							size="sm"
							className="h-7 px-2 text-[11px]"
							disabled={!instruction.trim() || pendingAction != null}
						>
							{t("conversation.nativeSubagent.control.send")}
						</Button>
					</div>
				</form>
			) : null}
		</div>
	);
}

function NativeSubagentTreeBranch({
	node,
	providers,
	depth,
	supervision,
}: {
	node: NativeSubagentTreeNode;
	providers?: ProviderCatalog["providers"];
	depth: number;
	supervision?: NativeSubagentSupervision;
}) {
	return (
		<div
			role="treeitem"
			aria-level={depth + 2}
			className="relative border-l border-border/60 pl-3"
		>
			<div className="absolute -left-px top-4 h-px w-3 bg-border/60" aria-hidden />
			{node.annotation ? (
				<NativeSubagentCard
					annotation={node.annotation}
					providers={providers}
					treeLabel={node.label}
					nested
					supervision={supervision}
				/>
			) : (
				<div className="flex min-h-8 items-center gap-2 rounded-md px-2 text-[12px] text-muted-foreground">
					<GitBranch className="size-3.5 shrink-0" aria-hidden />
					<span className="truncate font-medium text-foreground/80">{node.label}</span>
				</div>
			)}
			{node.children.length > 0 ? (
				<div className="ml-3 mt-1 flex flex-col gap-1">
					{node.children.map((child) => (
						<NativeSubagentTreeBranch
							key={child.key}
							node={child}
							providers={providers}
							depth={depth + 1}
							supervision={supervision}
						/>
					))}
				</div>
			) : null}
		</div>
	);
}

function NativeSubagentTree({
	annotations,
	providers,
	parentModelLabel,
	supervision,
}: {
	annotations: NativeSubagentAnnotation[];
	providers?: ProviderCatalog["providers"];
	parentModelLabel?: string | null;
	supervision?: NativeSubagentSupervision;
}) {
	const { t } = useTranslation("common");
	const projection = useMemo(
		() => projectNativeSubagentTree(annotations),
		[annotations],
	);

	return (
		<>
			{projection.roots.length > 0 ? (
				<div className="mb-2 rounded-lg border border-border/60 bg-muted/10 px-2.5 py-2">
					<div className="mb-2 flex items-center gap-2 text-[12px] text-muted-foreground">
						<GitBranch className="size-3.5 shrink-0" aria-hidden />
						<span className="font-medium text-foreground/85">
							{t("conversation.nativeSubagent.treeLabel")}
						</span>
						<span className="ml-auto text-[11px]">
							{t("conversation.nativeSubagent.treeCount", {
								count: projection.hierarchicalCount,
							})}
						</span>
					</div>
					<div role="tree" className="flex flex-col gap-1">
						<div
							role="treeitem"
							aria-level={1}
							className="flex min-h-8 items-center gap-2 rounded-md bg-muted/20 px-2 text-[12px]"
						>
							<Bot className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
							<span className="font-medium text-foreground/85">
								{t("conversation.nativeSubagent.principalAgent")}
							</span>
							{parentModelLabel ? (
								<span className="truncate text-muted-foreground">{parentModelLabel}</span>
							) : null}
						</div>
						<div className="ml-3 flex flex-col gap-1">
							{projection.roots.map((node) => (
								<NativeSubagentTreeBranch
									key={node.key}
									node={node}
									providers={providers}
									depth={0}
									supervision={supervision}
								/>
							))}
						</div>
					</div>
				</div>
			) : null}
			{projection.ungrouped.map((annotation) => (
				<NativeSubagentCard
					key={`native-subagent-${annotation.id}`}
					annotation={annotation}
					providers={providers}
					supervision={supervision}
				/>
			))}
		</>
	);
}

function AssistantActivityHistory({
	count,
	children,
}: {
	count: number;
	children: React.ReactNode;
}) {
	const { t } = useTranslation("common");
	const [isOpen, setIsOpen] = useState(false);

	return (
		<details
			className="flex min-w-0 flex-col"
			open={isOpen}
			onToggle={(event) => setIsOpen(event.currentTarget.open)}
		>
			<summary className="flex cursor-pointer items-center gap-1.5 rounded-md px-2 py-1 text-[12px] text-muted-foreground transition-colors hover:bg-muted/30 hover:text-foreground [&::-webkit-details-marker]:hidden">
				<ChevronRight
					className={cn("size-3 shrink-0 transition-transform", isOpen && "rotate-90")}
					aria-hidden
				/>
				<span>{t("conversation.activity.previous", { count })}</span>
			</summary>
			<div className="mt-1 flex min-w-0 flex-col gap-1.5 border-l border-border/45 pl-2">
				{children}
			</div>
		</details>
	);
}

function AssistantActivityGroup({
	annotations,
	turnStreaming,
	children,
}: {
	annotations: WorkspaceMessageAnnotation[];
	turnStreaming?: boolean;
	children: React.ReactNode;
}) {
	const { t } = useTranslation("common");
	const contentId = useId();
	const isLive = Boolean(turnStreaming) || annotations.some(
		(annotation) => Boolean(annotation.streaming),
	);
	const toolCount = annotations.filter((annotation) => annotation.type === "tool-call").length;
	const reasoningCount = annotations.filter(
		(annotation) => annotation.type === "reasoning",
	).length;
	const failedCount = annotations.filter(
		(annotation) => annotation.type === "tool-call" && annotation.status?.type === "failed",
	).length;
	const shouldStayOpen = shouldAutoOpenAssistantActivity(annotations, turnStreaming);
	const annotationChildren = Children.toArray(children);
	const { historyIndexes, prominentIndexes } = useMemo(
		() => partitionAssistantActivity(annotations),
		[annotations],
	);
	const initialOpenRef = useRef(shouldStayOpen);
	const [isOpen, setIsOpen] = useState(initialOpenRef.current);
	// Once the user toggles by hand, auto open/close stops driving this disclosure.
	const userToggledRef = useRef(false);
	const autoCollapseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

	useEffect(() => {
		if (autoCollapseTimerRef.current) {
			clearTimeout(autoCollapseTimerRef.current);
			autoCollapseTimerRef.current = null;
		}
		if (userToggledRef.current) {
			return;
		}
		if (shouldStayOpen) {
			setIsOpen(true);
			return;
		}
		if (!isOpen) {
			return;
		}
		autoCollapseTimerRef.current = setTimeout(() => {
			autoCollapseTimerRef.current = null;
			setIsOpen(false);
		}, ASSISTANT_ACTIVITY_AUTO_COLLAPSE_DELAY_MS);
		return () => {
			if (autoCollapseTimerRef.current) {
				clearTimeout(autoCollapseTimerRef.current);
				autoCollapseTimerRef.current = null;
			}
		};
	}, [isOpen, shouldStayOpen]);

	const showCompactedHistory =
		isLive || failedCount > 0 || (!userToggledRef.current && isOpen);
	const visibleContent = showCompactedHistory ? (
		<>
			{historyIndexes.length > 0 ? (
				<AssistantActivityHistory count={historyIndexes.length}>
					{historyIndexes.map((index) => annotationChildren[index])}
				</AssistantActivityHistory>
			) : null}
			{prominentIndexes.map((index) => annotationChildren[index])}
		</>
	) : (
		children
	);

	const handleToggle = () => {
		userToggledRef.current = true;
		if (autoCollapseTimerRef.current) {
			clearTimeout(autoCollapseTimerRef.current);
			autoCollapseTimerRef.current = null;
		}
		setIsOpen((open) => !open);
	};

	return (
		<div
			className="mb-2 flex min-w-0 flex-col rounded-lg border border-border/50 bg-muted/15 px-2.5 py-2"
			data-state={isOpen ? "open" : "closed"}
		>
			<button
				type="button"
				aria-expanded={isOpen}
				aria-controls={contentId}
				onClick={handleToggle}
				className="flex w-full cursor-pointer items-center gap-2 text-left text-[12px] text-muted-foreground"
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
			</button>
			<div
				id={contentId}
				aria-hidden={!isOpen}
				inert={!isOpen}
				className={cn(
					"grid transition-[grid-template-rows,opacity] duration-300 ease-out motion-reduce:transition-none",
					isOpen
						? "grid-rows-[1fr] opacity-100"
						: "pointer-events-none grid-rows-[0fr] opacity-0",
				)}
			>
				<div className="min-h-0 overflow-hidden">
					<div className="mt-2 flex min-w-0 flex-col gap-1.5 pl-1">
						{visibleContent}
					</div>
				</div>
			</div>
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
	isPlanApproved,
	isPlanReadOnly,
	sessionId,
	providers,
	providerId,
	modelId,
	activeMissionSpecRelativePath,
	activeMissionSpecHash,
	autoSaveMissionValidation,
	onDelegateTaskApprove,
	onContinue,
	onRetry,
	onFork,
	onOpenPlan,
	onOpenFileReference,
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
	providers?: ProviderCatalog["providers"];
	providerId?: string | null;
	modelId?: string | null;
	activeMissionSpecRelativePath?: string | null;
	activeMissionSpecHash?: string | null;
	autoSaveMissionValidation?: boolean;
	onDelegateTaskApprove?: (request: AgentInitiatedDelegationRequest) => Promise<void>;
	onContinue?: () => void;
	/** Re-runs the aborted turn with the same prompt, linked as an explicit retry. */
	onRetry?: () => void;
	/** Starts a new thread anchored on this reply and everything before it. */
	onFork?: () => void;
	onOpenPlan?: () => void;
	onOpenFileReference?: (reference: WorkspaceFileReference) => void;
	hidePendingApprovals?: boolean;
}) {
	const { t } = useTranslation("common");
	const modelLabel = resolveModelLabel(modelId, providers);
	const provider = providers?.find((candidate) => candidate.id === providerId);
	const nativeSubagentSupervision = useMemo<NativeSubagentSupervision>(
		() => ({
			sessionId,
			parentStreaming: streaming,
			supportsSteering:
				provider?.capabilities.supportsNativeSubagentSteering ?? false,
			supportsInterrupt:
				provider?.capabilities.supportsNativeSubagentInterrupt ?? false,
		}),
		[provider, sessionId, streaming],
	);
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
	const nativeSubagentAnnotations = useMemo(
		() =>
			(annotations ?? []).filter(
				(annotation): annotation is NativeSubagentAnnotation =>
					annotation.type === "native-subagent",
			),
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
	const hasAssistantText = content.trim().length > 0;
	const isValidationStale = Boolean(
		validationReport?.specHash &&
			activeMissionSpecHash &&
		validationReport.specHash !== activeMissionSpecHash,
	);
	const handleOpenFileReference = useCallback(
		(reference: WorkspaceFileReference) => {
			onOpenFileReference?.(reference);
		},
		[onOpenFileReference],
	);
	const getFileReferenceTitle = useCallback(
		(reference: WorkspaceFileReference) =>
			reference.line
				? t("conversation.fileReference.openAtPosition", {
						path: reference.path,
						position: reference.column
							? `${reference.line}:${reference.column}`
							: String(reference.line),
					})
				: t("conversation.fileReference.open", { path: reference.path }),
		[t],
	);
	return (
		<WorkspaceFileLinkProvider
			workspaceRoot={onOpenFileReference ? (workspacePath ?? null) : null}
			onOpenFile={handleOpenFileReference}
			getTitle={getFileReferenceTitle}
		>
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
				{modelLabel ? (
					<div className="mb-2 flex items-center gap-1.5 text-[11px] text-muted-foreground">
						<Bot className="size-3.5 shrink-0" aria-hidden />
						<span>{t("conversation.modelLabel")}:</span>
						<span className="font-medium text-foreground/80">{modelLabel}</span>
					</div>
				) : null}
				{activityAnnotations.length ? (
					<AssistantActivityGroup
						annotations={activityAnnotations}
						turnStreaming={streaming}
					>
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
				{nativeSubagentAnnotations.length > 0 ? (
					<NativeSubagentTree
						annotations={nativeSubagentAnnotations}
						providers={providers}
						parentModelLabel={modelLabel}
						supervision={nativeSubagentSupervision}
					/>
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
				) : hasAssistantText ? (
					<div className={cn("assistant-markdown-scale max-w-none break-words text-foreground")}>
						<Suspense fallback={<AssistantTextFallback text={content} />}>
							<LazyStreamdown
								mode={streaming ? "streaming" : "static"}
								animated={assistantStreamingAnimation(streaming, content.length)}
								caret={streaming ? "block" : undefined}
								className="conversation-streamdown"
								isAnimating={Boolean(streaming)}
								shikiTheme={ASSISTANT_STREAMDOWN_SHIKI_THEME}
							>
								{content}
							</LazyStreamdown>
						</Suspense>
					</div>
				) : null}
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
						{onRetry ? (
							<Button
								type="button"
								variant="outline"
								size="sm"
								className="h-7 shrink-0 gap-1.5 px-2 text-[11px]"
								onClick={onRetry}
								title={t("conversation.message.retryHint")}
							>
								<RotateCcw className="size-3" aria-hidden />
								{t("conversation.message.retry")}
							</Button>
						) : null}
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
						{onFork ? (
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								aria-label={t("conversation.message.forkAssistant")}
								title={t("conversation.message.forkAssistant")}
								className={cn(
									"pointer-events-auto size-5 shrink-0 text-muted-foreground/28 hover:text-muted-foreground",
									"bg-transparent hover:bg-transparent",
								)}
								onClick={onFork}
							>
								<GitFork className="size-3.5" aria-hidden />
							</Button>
						) : null}
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
		</WorkspaceFileLinkProvider>
	);
}
