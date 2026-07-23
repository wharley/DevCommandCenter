import { Suspense, useId, useRef, useState } from "react";
import {
	Check,
	Circle,
	Copy,
	Download,
	Ellipsis,
	Loader2,
	MessageSquarePlus,
	Save,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { LazyStreamdown } from "@/components/streamdown-loader";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogContent,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import {
	buildCollapsedPlanPreviewMarkdown,
	buildPlanMarkdownFilename,
	buildPlanRevisionPrompt,
	downloadPlanAsTextFile,
	normalizePlanContentForExport,
	stripDisplayedPlanMarkdown,
	type ParsedPlanContent,
} from "../plan-content";
import type { MissionAcceptanceCriterionCoverage } from "@/features/spec/mission-spec-content";

type PlanReviewCardProps = {
	plan: ParsedPlanContent;
	acceptanceCriteriaCoverage?: MissionAcceptanceCriterionCoverage[];
	workspacePath?: string | null;
	className?: string;
	onRequestRevision?: (prompt: string) => void;
	documentMode?: boolean;
};

function stepIcon(status: ParsedPlanContent["steps"][number]["status"]) {
	if (status === "completed") {
		return <Check className="size-3.5 text-emerald-500" strokeWidth={2} />;
	}
	if (status === "in_progress") {
		return <Loader2 className="size-3.5 animate-spin text-sky-500" strokeWidth={2} />;
	}
	return <Circle className="size-3.5 text-muted-foreground/50" strokeWidth={2} />;
}

function stepTextClass(status: ParsedPlanContent["steps"][number]["status"]) {
	if (status === "completed") {
		return "text-muted-foreground/55 line-through decoration-muted-foreground/25";
	}
	if (status === "in_progress") {
		return "text-foreground";
	}
	return "text-muted-foreground/85";
}

function savePlanToWorkspace(
	workspacePath: string,
	relativePath: string,
	contents: string,
) {
	const api = window as Window & {
		desktopAPI?: {
			worktree?: {
				writeFile?: (
					worktreePath: string,
					relativePath: string,
					content: string,
				) => Promise<{ success: boolean; relativePath?: string; error?: string }>;
			};
		};
	};
	return (
		api.desktopAPI?.worktree?.writeFile?.(workspacePath, relativePath, contents) ??
		Promise.resolve({
			success: false,
			error: "Workspace file API unavailable",
		})
	);
}

export function PlanReviewCard({
	plan,
	acceptanceCriteriaCoverage = [],
	workspacePath,
	className,
	onRequestRevision,
	documentMode = false,
}: PlanReviewCardProps) {
	const [expanded, setExpanded] = useState(false);
	const [isSaveDialogOpen, setIsSaveDialogOpen] = useState(false);
	const [isRevisionDialogOpen, setIsRevisionDialogOpen] = useState(false);
	const [savePath, setSavePath] = useState("");
	const [selectedText, setSelectedText] = useState("");
	const [selectionActionPosition, setSelectionActionPosition] = useState<{
		top: number;
		left: number;
	} | null>(null);
	const [revisionComment, setRevisionComment] = useState("");
	const [isSavingToWorkspace, setIsSavingToWorkspace] = useState(false);
	const cardRef = useRef<HTMLDivElement>(null);
	const hasSteps = plan.steps.length > 0;
	const completedSteps = plan.steps.filter((step) => step.status === "completed").length;
	const coveredCriteria = acceptanceCriteriaCoverage.filter(
		(criterion) => criterion.covered,
	).length;
	const canCollapse = plan.canCollapse;
	const exportContents = normalizePlanContentForExport(plan.markdown);
	const exportFilename = buildPlanMarkdownFilename(plan.markdown);
	const displayedPlanMarkdown = stripDisplayedPlanMarkdown(plan.markdown);
	const collapsedPreview = canCollapse
		? buildCollapsedPlanPreviewMarkdown(plan.markdown, { maxLines: 10 })
		: null;
	const workspaceFilePath = exportFilename;
	const savePathInputId = useId();
	const revisionCommentInputId = useId();
	const { t } = useTranslation("common");

	const captureSelection = () => {
		if (!onRequestRevision) return;
		const selection = window.getSelection();
		const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
		const excerpt = selection?.toString().trim() ?? "";
		if (!range || !excerpt || !cardRef.current?.contains(range.commonAncestorContainer)) {
			return;
		}
		const selectionRect = range.getBoundingClientRect();
		const cardRect = cardRef.current.getBoundingClientRect();
		const actionWidth = 148;
		setSelectedText(excerpt);
		setSelectionActionPosition({
			top: selectionRect.bottom - cardRect.top + 8,
			left: Math.max(
				12,
				Math.min(
					selectionRect.left - cardRect.left,
					cardRect.width - actionWidth - 12,
				),
			),
		});
	};

	const handleCopy = async () => {
		try {
			await navigator.clipboard.writeText(exportContents);
			toast.success("Plan copied");
		} catch (error) {
			toast.error(
				error instanceof Error ? error.message : "Unable to copy the plan.",
			);
		}
	};

	const handleDownload = () => {
		downloadPlanAsTextFile(exportFilename, exportContents);
		toast.success("Plan downloaded");
	};

	const openSaveDialog = () => {
		setSavePath((current) => (current.length > 0 ? current : workspaceFilePath));
		setIsSaveDialogOpen(true);
	};

	const handleSave = () => {
		if (!workspacePath) {
			toast.error("Workspace path is unavailable");
			return;
		}
		const relativePath = savePath.trim();
		if (!relativePath) {
			toast.error("Enter a workspace path");
			return;
		}

		setIsSavingToWorkspace(true);
		void savePlanToWorkspace(workspacePath, relativePath, exportContents)
			.then((result: { success: boolean; relativePath?: string; error?: string }) => {
					if (!result?.success) {
						throw new Error(result?.error ?? "Unable to save plan.");
					}
					toast.success("Plan saved to workspace", {
						description: result?.relativePath ?? relativePath,
					});
				})
				.catch((error: unknown) => {
					toast.error(
						error instanceof Error ? error.message : "Unable to save the plan.",
					);
				})
			.finally(() => {
				setIsSavingToWorkspace(false);
				setIsSaveDialogOpen(false);
			});
	};

	const handleSubmitRevision = () => {
		const comment = revisionComment.trim();
		if (!selectedText || !comment || !onRequestRevision) {
			return;
		}
		onRequestRevision(
			buildPlanRevisionPrompt({
				planMarkdown: plan.markdown,
				selectedText,
				comment,
			}),
		);
		setRevisionComment("");
		setSelectedText("");
		setSelectionActionPosition(null);
		setIsRevisionDialogOpen(false);
		toast.success(t("planSurface.revisionRequested"));
	};

	return (
		<div
			ref={cardRef}
			onMouseUp={captureSelection}
			className={cn(
				"relative rounded-[22px] border border-border/70 bg-card/75 p-4 shadow-[0_12px_40px_rgba(0,0,0,0.06)] backdrop-blur-sm",
				className,
			)}
		>
			{selectedText && selectionActionPosition ? (
				<Button
					type="button"
					size="sm"
					className="absolute z-20 h-8 gap-1.5 rounded-full px-3 shadow-lg"
					style={selectionActionPosition}
					onMouseDown={(event) => event.preventDefault()}
					onClick={() => setIsRevisionDialogOpen(true)}
				>
					<MessageSquarePlus className="size-3.5" aria-hidden />
					{t("planSurface.commentSelection")}
				</Button>
			) : null}
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div className="min-w-0">
					<div className="flex items-center gap-2">
						<Badge
							variant="secondary"
							className="rounded-md px-2 py-0 text-[10px] font-semibold uppercase tracking-[0.08em]"
						>
							Plan
						</Badge>
						<p className="truncate text-sm font-medium text-foreground">
							{plan.title}
						</p>
					</div>
					<p className="mt-1 text-[11px] leading-5 text-muted-foreground">
						{acceptanceCriteriaCoverage.length > 0
							? `${coveredCriteria}/${acceptanceCriteriaCoverage.length} criteria covered`
							: hasSteps
							? `${completedSteps}/${plan.steps.length} steps tracked`
							: "Structured plan preview"}
					</p>
				</div>

				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button
							type="button"
							variant="outline"
							size="icon-xs"
							aria-label="Plan actions"
						>
							<Ellipsis className="size-3.5" aria-hidden />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end" className="min-w-52">
						<DropdownMenuItem onClick={() => void handleCopy()}>
							<Copy className="size-3.5" />
							Copy to clipboard
						</DropdownMenuItem>
						<DropdownMenuItem onClick={handleDownload}>
							<Download className="size-3.5" />
							Download as markdown
						</DropdownMenuItem>
						<DropdownMenuItem
							onClick={openSaveDialog}
							disabled={!workspacePath}
						>
							<Save className="size-3.5" />
							Save as markdown
						</DropdownMenuItem>
					</DropdownMenuContent>
				</DropdownMenu>
			</div>

			{plan.summary && !documentMode ? (
				<div className="mt-4">
					<p className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						Summary
					</p>
					<div className="conversation-markdown max-w-none break-words text-[13px] leading-6 text-foreground">
						<Suspense fallback={<p className="whitespace-pre-wrap">{plan.summary}</p>}>
							<LazyStreamdown className="conversation-streamdown" mode="static">
								{plan.summary}
							</LazyStreamdown>
						</Suspense>
					</div>
				</div>
			) : null}

			{acceptanceCriteriaCoverage.length > 0 ? (
				<div className="mt-4">
					<p className="mb-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						Acceptance criteria coverage
					</p>
					<div className="grid gap-1.5">
						{acceptanceCriteriaCoverage.map((criterion) => (
							<div
								key={criterion.id}
								className={cn(
									"rounded-xl border px-2.5 py-2",
									criterion.covered
										? "border-emerald-500/25 bg-emerald-500/5"
										: "border-border/50 bg-muted/10",
								)}
							>
								<div className="flex items-center gap-2">
									{criterion.covered ? (
										<Check className="size-3.5 text-emerald-500" strokeWidth={2} />
									) : (
										<Circle className="size-3.5 text-muted-foreground/50" strokeWidth={2} />
									)}
									<span className="font-mono text-[11px] font-semibold text-foreground">
										{criterion.id}
									</span>
									<span className="text-[10px] uppercase tracking-[0.08em] text-muted-foreground">
										{criterion.covered ? "covered" : "uncovered"}
									</span>
								</div>
								{criterion.description ? (
									<p className="mt-1 pl-5 text-[12px] leading-5 text-muted-foreground">
										{criterion.description}
									</p>
								) : null}
							</div>
						))}
					</div>
				</div>
			) : null}

			{hasSteps && !documentMode ? (
				<div className="mt-4">
					<p className="mb-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						Steps
					</p>
					<div className="grid gap-1.5">
						{plan.steps.map((step) => (
							<div
								key={`${step.index}-${step.text}`}
								className={cn(
									"rounded-xl border border-border/50 px-2.5 py-2",
									step.status === "completed" && "bg-emerald-500/5",
									step.status === "in_progress" && "bg-sky-500/5",
								)}
							>
								<div className="flex items-start gap-2">
									<div className="mt-0.5 shrink-0">{stepIcon(step.status)}</div>
									<p
										className={cn(
											"text-[13px] leading-snug",
											stepTextClass(step.status),
										)}
									>
										{step.text}
									</p>
								</div>
								{step.criteria.length > 0 ? (
									<div className="mt-2 flex flex-wrap gap-1 pl-[1.35rem]">
										{step.criteria.map((criterion) => (
											<Badge
												key={`${step.index}-${criterion}`}
												variant="outline"
												className="h-5 rounded-md px-1.5 text-[10px] font-mono"
											>
												{criterion}
											</Badge>
										))}
									</div>
								) : null}
							</div>
						))}
					</div>
				</div>
			) : null}

			{plan.approvedPrompts.length > 0 && !documentMode ? (
				<div className="mt-4">
					<p className="mb-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						Approved prompts
					</p>
					<div className="grid gap-2 rounded-2xl border border-border/50 bg-muted/20 p-2.5">
						{plan.approvedPrompts.map((prompt) => (
							<div
								key={prompt.id}
								className="rounded-xl border border-border/50 bg-background/85 px-2.5 py-2"
							>
								<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
									{prompt.source}
								</p>
								<p className="mt-1 whitespace-pre-wrap break-words text-[12px] leading-5 text-foreground">
									{prompt.command}
								</p>
								{prompt.description ? (
									<p className="mt-1 text-[11px] leading-5 text-muted-foreground">
										{prompt.description}
									</p>
								) : null}
							</div>
						))}
					</div>
				</div>
			) : null}

			{documentMode ? (
				<div className="mt-5 border-t border-border/50 pt-5">
					<div className="conversation-markdown mx-auto max-w-[78ch] break-words text-[14px] leading-7 text-foreground">
						<Suspense
							fallback={
								<pre className="whitespace-pre-wrap break-words font-sans text-[14px] leading-7">
									{displayedPlanMarkdown}
								</pre>
							}
						>
							<LazyStreamdown className="conversation-streamdown" mode="static">
								{displayedPlanMarkdown}
							</LazyStreamdown>
						</Suspense>
					</div>
				</div>
			) : canCollapse ? (
				<div className="mt-4">
					<div className={cn("relative", !expanded && "max-h-104 overflow-hidden")}>
						<div className="conversation-markdown max-w-none break-words text-[13px] leading-6 text-foreground">
							<Suspense
								fallback={
									<pre className="whitespace-pre-wrap break-words text-[12px] leading-6">
										{collapsedPreview ?? displayedPlanMarkdown}
									</pre>
								}
							>
								<LazyStreamdown className="conversation-streamdown" mode="static">
									{expanded ? displayedPlanMarkdown : collapsedPreview ?? displayedPlanMarkdown}
								</LazyStreamdown>
							</Suspense>
						</div>
						{!expanded ? (
							<div className="pointer-events-none absolute inset-x-0 bottom-0 h-24 bg-linear-to-t from-card/95 via-card/80 to-transparent" />
						) : null}
					</div>
					<div className="mt-4 flex justify-center">
						<Button
							type="button"
							variant="outline"
							size="sm"
							data-scroll-anchor-ignore
							onClick={() => setExpanded((value) => !value)}
						>
							{expanded ? "Collapse plan" : "Expand plan"}
						</Button>
					</div>
				</div>
			) : plan.isPlanLike ? null : (
				<div className="mt-4 rounded-2xl border border-dashed border-border/60 bg-muted/10 p-3">
					<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
						Plan preview
					</p>
					<p className="mt-1 text-[12px] leading-5 text-muted-foreground">
						No structured steps were detected. The raw assistant response is still
						available in the full plan view.
					</p>
				</div>
			)}

			<Dialog
				open={isRevisionDialogOpen}
				onOpenChange={setIsRevisionDialogOpen}
			>
				<DialogContent className="max-w-xl">
					<DialogHeader>
						<DialogTitle>{t("planSurface.commentTitle")}</DialogTitle>
						<DialogDescription>
							{t("planSurface.commentDescription")}
						</DialogDescription>
					</DialogHeader>
					<div className="grid gap-3">
						<div className="rounded-lg border border-border/60 bg-muted/30 p-3">
							<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
								{t("planSurface.selectedText")}
							</p>
							<p className="mt-1 whitespace-pre-wrap text-sm leading-5 text-foreground">
								{selectedText}
							</p>
						</div>
						<label htmlFor={revisionCommentInputId} className="grid gap-1.5">
							<span className="text-sm font-medium text-foreground">
								{t("planSurface.yourComment")}
							</span>
							<Textarea
								id={revisionCommentInputId}
								value={revisionComment}
								onChange={(event) => setRevisionComment(event.target.value)}
								placeholder={t("planSurface.commentPlaceholder")}
								autoFocus
							/>
						</label>
					</div>
					<DialogFooter>
						<Button type="button" variant="outline" onClick={() => setIsRevisionDialogOpen(false)}>
							{t("planSurface.cancel")}
						</Button>
						<Button
							type="button"
							disabled={revisionComment.trim().length === 0}
							onClick={handleSubmitRevision}
						>
							{t("planSurface.requestRevision")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>

			<Dialog
				open={isSaveDialogOpen}
				onOpenChange={(open) => {
					if (!isSavingToWorkspace) {
						setIsSaveDialogOpen(open);
					}
				}}
			>
					<DialogContent className="max-w-xl" showCloseButton={false}>
						<DialogHeader>
							<DialogTitle>Save plan as markdown</DialogTitle>
							<DialogDescription>
							Enter a path relative to the workspace root.
							</DialogDescription>
						</DialogHeader>
					<div className="space-y-3">
						<label htmlFor={savePathInputId} className="grid gap-1.5">
							<span className="text-xs font-medium text-foreground">Workspace path</span>
							<Input
								id={savePathInputId}
								value={savePath}
								onChange={(event) => setSavePath(event.target.value)}
								placeholder={workspaceFilePath}
								spellCheck={false}
								disabled={isSavingToWorkspace}
							/>
						</label>
					</div>
					<DialogFooter>
						<Button
							variant="outline"
							size="sm"
							onClick={() => setIsSaveDialogOpen(false)}
							disabled={isSavingToWorkspace}
						>
							Cancel
						</Button>
						<Button
							size="sm"
							onClick={() => void handleSave()}
							disabled={isSavingToWorkspace}
						>
							{isSavingToWorkspace ? "Saving..." : "Save"}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</div>
	);
}
