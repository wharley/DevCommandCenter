import { ExternalLink, MessageSquare, Send, X } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { TrafficLightSpacer } from "@/components/chrome/traffic-light-spacer";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { shouldIgnoreGlobalShortcutTarget } from "@/features/shortcuts/shortcut-utils";
import { ShortcutDisplay } from "@/features/shortcuts/shortcut-display";
import { InlineShortcutDisplay } from "@/features/shortcuts/InlineShortcutDisplay";
import { useWorkspaceGitFilePreviewContent } from "@/features/inspector/use-workspace-git-file-preview-content";
import { openExternal } from "@/lib/shell-api";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";
import type { WorkspacePrReviewComment } from "@dcc/contracts";
import type {
	DiffAnnotationPayload,
	DiffMachineAnnotation,
} from "@/lib/monaco-runtime";
import {
	DiffAnnotationPopover,
	type DiffAnnotationRequest,
	type DiffAnnotationSubmit,
	type PendingAnnotation,
} from "./diff-annotation";

// Re-exported for backward compatibility with existing importers.
export type { DiffAnnotationRequest, DiffAnnotationSubmit };

type WorkspaceEditorSurfaceProps = {
	workspaceRoot: string | null;
	selection: WorkspaceGitPreviewSelection;
	onClose: () => void;
	/** Send the annotated selection + instruction to an agent. */
	onSubmitAnnotation?: (input: DiffAnnotationSubmit) => void;
	/** Load the annotation into the composer draft for manual refinement. */
	onEditInComposer?: (input: {
		request: DiffAnnotationRequest;
		instruction: string;
	}) => void;
	/** Collect the annotation into the multi-snippet review buffer. */
	onAddToReview?: (input: {
		request: DiffAnnotationRequest;
		note: string;
	}) => void;
};

type MonacoRuntimeModule = typeof import("@/lib/monaco-runtime");
type MonacoDiffController = Awaited<
	ReturnType<MonacoRuntimeModule["createDiffEditor"]>
>;

function WorkspaceEditorDiff({
	path,
	originalText,
	modifiedText,
	inline,
	focusLine,
	machineAnnotations,
	onAnnotate,
	annotateLabel,
	onMachineAnnotationClick,
	reviewCommentLabel,
}: {
	path: string;
	originalText: string;
	modifiedText: string;
	inline: boolean;
	focusLine?: number | null;
	machineAnnotations?: DiffMachineAnnotation[];
	onAnnotate?: (payload: DiffAnnotationPayload) => void;
	annotateLabel: string;
	onMachineAnnotationClick?: (input: {
		annotation: DiffMachineAnnotation;
		anchor: { top: number; left: number };
	}) => void;
	reviewCommentLabel: string;
}) {
	const hostRef = useRef<HTMLDivElement | null>(null);
	const controllerRef = useRef<MonacoDiffController | null>(null);
	const requestIdRef = useRef(0);
	// Keep the latest callback/label in refs so they never recreate the editor.
	const onAnnotateRef = useRef(onAnnotate);
	onAnnotateRef.current = onAnnotate;
	const annotateLabelRef = useRef(annotateLabel);
	annotateLabelRef.current = annotateLabel;
	const onMachineAnnotationClickRef = useRef(onMachineAnnotationClick);
	onMachineAnnotationClickRef.current = onMachineAnnotationClick;
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);

	useEffect(
		() => () => {
			controllerRef.current?.dispose();
			controllerRef.current = null;
		},
		[],
	);

	useLayoutEffect(() => {
		const host = hostRef.current;
		if (!host) return;

		const requestId = requestIdRef.current + 1;
		requestIdRef.current = requestId;
		let disposed = false;

		controllerRef.current?.dispose();
		controllerRef.current = null;
		host.replaceChildren();
		setLoading(true);
		setError(null);

		void (async () => {
			try {
				const { createDiffEditor } = await import("@/lib/monaco-runtime");
				const controller = await createDiffEditor({
					container: host,
					path,
					originalText,
					modifiedText,
					inline,
					focusLine,
					machineAnnotations,
					onAnnotate: (payload) => onAnnotateRef.current?.(payload),
					annotateLabel: annotateLabelRef.current,
					onMachineAnnotationClick: (payload) =>
						onMachineAnnotationClickRef.current?.(payload),
					reviewCommentLabel,
				});

				if (disposed || requestId !== requestIdRef.current) {
					controller.dispose();
					return;
				}

				controllerRef.current = controller;
				setLoading(false);
				requestAnimationFrame(() => {
					if (disposed || requestId !== requestIdRef.current) {
						return;
					}
					const modified = controller.editor.getModifiedEditor();
					modified.layout();
					modified.focus();
				});
			} catch (cause) {
				if (disposed) return;
				setError(cause instanceof Error ? cause.message : "Failed to load editor");
				setLoading(false);
			}
		})();

		return () => {
			disposed = true;
		};
	}, [focusLine, inline, machineAnnotations, modifiedText, originalText, path, reviewCommentLabel]);

	useEffect(() => {
		controllerRef.current?.setTexts({
			originalText,
			modifiedText,
			inline,
		});
	}, [inline, modifiedText, originalText]);

	useEffect(() => {
		controllerRef.current?.setMachineAnnotations(machineAnnotations ?? []);
	}, [machineAnnotations]);

	useEffect(() => {
		if (focusLine) {
			controllerRef.current?.revealLine(focusLine);
		}
	}, [focusLine]);

	return (
		<div className="relative flex min-h-0 flex-1 overflow-hidden bg-background">
			<div ref={hostRef} className="h-full min-h-0 flex-1" />
			{loading ? (
				<div className="pointer-events-none absolute inset-0 flex items-center justify-center bg-background/70">
					<span className="text-[11px] text-muted-foreground">Loading editor...</span>
				</div>
			) : null}
			{error ? (
				<div className="absolute inset-0 flex items-center justify-center bg-background">
					<p className="text-[11px] text-destructive">{error}</p>
				</div>
			) : null}
		</div>
	);
}

type ReviewThread = {
	id: number;
	comments: WorkspacePrReviewComment[];
};

function reviewCommentLine(comment: WorkspacePrReviewComment): number | null {
	const side = comment.side?.toUpperCase();
	if (side === "LEFT") {
		return comment.originalLine ?? comment.originalStartLine ?? comment.line;
	}
	return comment.line ?? comment.startLine ?? comment.originalLine;
}

function reviewCommentStartLine(comment: WorkspacePrReviewComment): number | null {
	const side = comment.side?.toUpperCase();
	if (side === "LEFT") {
		return comment.originalStartLine ?? comment.originalLine ?? comment.line;
	}
	return comment.startLine ?? comment.line ?? comment.originalLine;
}

function reviewCommentSide(comment: WorkspacePrReviewComment): "original" | "modified" {
	return comment.side?.toUpperCase() === "LEFT" ? "original" : "modified";
}

function buildReviewThreads(comments: WorkspacePrReviewComment[]): ReviewThread[] {
	const grouped = new Map<number, WorkspacePrReviewComment[]>();
	for (const comment of comments) {
		const thread = grouped.get(comment.threadId);
		if (thread) {
			thread.push(comment);
		} else {
			grouped.set(comment.threadId, [comment]);
		}
	}
	return [...grouped.entries()]
		.map(([id, threadComments]) => ({
			id,
			comments: [...threadComments].sort((left, right) => {
				const leftTime = Date.parse(left.createdAt ?? "");
				const rightTime = Date.parse(right.createdAt ?? "");
				return (Number.isNaN(leftTime) ? 0 : leftTime) - (Number.isNaN(rightTime) ? 0 : rightTime);
			}),
		}))
		.sort((left, right) => {
			const leftLine = reviewCommentLine(left.comments[0]!) ?? 0;
			const rightLine = reviewCommentLine(right.comments[0]!) ?? 0;
			return leftLine - rightLine;
		});
}

function buildReviewCommentAnnotations(
	threads: ReviewThread[],
): DiffMachineAnnotation[] {
	return threads.map((thread) => {
		const first = thread.comments[0]!;
		const startLine = reviewCommentStartLine(first) ?? reviewCommentLine(first) ?? 1;
		const endLine = reviewCommentLine(first) ?? startLine;
		const author = first.author?.login ? `@${first.author.login}` : "Reviewer";
		const title =
			first.body.split("\n").map((line) => line.trim()).find(Boolean) ??
			"Review comment";
		return {
			source: "github-review",
			id: String(thread.id),
			severity: "info",
			side: reviewCommentSide(first),
			startLine,
			endLine: Math.max(startLine, endLine),
			title: `${author}: ${title}`,
		};
	});
}

function snippetFromLines(text: string, startLine: number, endLine: number): string {
	const lines = text.split(/\r?\n/);
	const start = Math.max(0, startLine - 1);
	const end = Math.min(lines.length, Math.max(startLine, endLine));
	return lines.slice(start, end).join("\n");
}

function buildReviewThreadPrompt({
	selection,
	thread,
	snippet,
}: {
	selection: WorkspaceGitPreviewSelection;
	thread: ReviewThread;
	snippet: string;
}) {
	const first = thread.comments[0]!;
	const line = reviewCommentLine(first);
	const side = reviewCommentSide(first);
	const comments = thread.comments
		.map((comment) => {
			const author = comment.author?.login ? `@${comment.author.login}` : "Reviewer";
			return `${author}:\n${comment.body.trim()}`;
		})
		.join("\n\n");
	return [
		`Analyze this GitHub PR review thread and implement the appropriate fix.`,
		"",
		`File: ${selection.path}`,
		line ? `Line: ${line}` : null,
		`Side: ${side}`,
		selection.baseBranch ? `Base branch: ${selection.baseBranch}` : null,
		first.htmlUrl ? `GitHub: ${first.htmlUrl}` : null,
		"",
		"Review thread:",
		comments,
		"",
		"Relevant code:",
		"```",
		snippet,
		"```",
	]
		.filter((linePart): linePart is string => linePart != null)
		.join("\n");
}

/**
 * Largest-unit relative timestamp ("2h ago", "yesterday"), localized via the
 * active i18n language. Falls back to null for unparseable / missing dates so
 * the caller can hide the slot entirely.
 */
const REVIEW_RELATIVE_UNITS: Array<[Intl.RelativeTimeFormatUnit, number]> = [
	["year", 31_557_600],
	["month", 2_629_800],
	["week", 604_800],
	["day", 86_400],
	["hour", 3_600],
	["minute", 60],
];

function formatReviewTimestamp(iso: string | null, locale: string): string | null {
	if (!iso) return null;
	const target = Date.parse(iso);
	if (Number.isNaN(target)) return null;
	const deltaSeconds = (target - Date.now()) / 1000;
	const formatter = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
	if (Math.abs(deltaSeconds) < 45) {
		return formatter.format(0, "second");
	}
	for (const [unit, secondsInUnit] of REVIEW_RELATIVE_UNITS) {
		if (Math.abs(deltaSeconds) >= secondsInUnit) {
			return formatter.format(Math.round(deltaSeconds / secondsInUnit), unit);
		}
	}
	return formatter.format(Math.round(deltaSeconds / 60), "minute");
}

function reviewAuthorInitials(login: string | null | undefined): string {
	const handle = login?.trim();
	if (!handle) return "?";
	return handle.slice(0, 2).toUpperCase();
}

function ReviewThreadDrawer({
	selection,
	thread,
	snippet,
	onClose,
	onSendToAgent,
	onEditInComposer,
}: {
	selection: WorkspaceGitPreviewSelection;
	thread: ReviewThread;
	snippet: string;
	onClose: () => void;
	onSendToAgent: () => void;
	onEditInComposer?: () => void;
}) {
	const { t, i18n } = useTranslation("common");
	const first = thread.comments[0]!;
	const line = reviewCommentLine(first);
	const githubUrl =
		first.htmlUrl ?? thread.comments.find((comment) => comment.htmlUrl)?.htmlUrl ?? null;
	const reviewerLabel = t("reviewThread.reviewer");
	const multiple = thread.comments.length > 1;

	return (
		<aside
			role="dialog"
			aria-label={t("reviewThread.title")}
			className="absolute right-3 top-12 z-30 flex max-h-[min(72vh,580px)] w-[368px] max-w-[calc(100%-1.5rem)] flex-col overflow-hidden rounded-xl border border-border/80 bg-popover text-popover-foreground shadow-xl ring-1 ring-foreground/10 animate-in fade-in-0 zoom-in-95 duration-100"
		>
			<header className="flex items-center gap-2.5 border-b border-border/60 bg-muted/30 px-3 py-2.5">
				<span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary ring-1 ring-inset ring-primary/15">
					<MessageSquare className="size-3.5" strokeWidth={2.2} />
				</span>
				<div className="min-w-0 flex-1">
					<div className="flex items-center gap-1.5">
						<p className="truncate text-[12px] font-semibold leading-tight">
							{t("reviewThread.title")}
						</p>
						{multiple ? (
							<span className="shrink-0 rounded-full bg-muted px-1.5 text-[9.5px] font-semibold leading-[15px] text-muted-foreground tabular-nums">
								{thread.comments.length}
							</span>
						) : null}
					</div>
					<p
						className="truncate font-mono text-[10px] leading-tight text-muted-foreground"
						title={selection.path}
						dir="rtl"
					>
						{selection.path}{line ? `:${line}` : ""}
					</p>
				</div>
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					className="size-6 text-muted-foreground hover:text-foreground"
					onClick={onClose}
					aria-label={t("reviewThread.close")}
				>
					<X className="size-3.5" strokeWidth={2} />
				</Button>
			</header>

			<div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
				<ol className="relative space-y-3.5">
					{multiple ? (
						<span
							aria-hidden
							className="absolute bottom-2 left-[11px] top-2 w-px bg-border/70"
						/>
					) : null}
					{thread.comments.map((comment) => {
						const login = comment.author?.login ?? null;
						const displayName = login ? `@${login}` : reviewerLabel;
						const timestamp = formatReviewTimestamp(comment.createdAt, i18n.language);
						return (
							<li key={comment.id} className="relative pl-9">
								<span className="absolute left-0 top-0 flex size-6 items-center justify-center overflow-hidden rounded-full bg-muted text-[9px] font-semibold text-muted-foreground ring-2 ring-popover">
									{comment.author?.avatarUrl ? (
										<img
											src={comment.author.avatarUrl}
											alt=""
											className="size-full object-cover"
										/>
									) : (
										reviewAuthorInitials(login)
									)}
								</span>
								<div className="flex items-baseline gap-1.5">
									<span className="truncate text-[11.5px] font-semibold text-foreground">
										{displayName}
									</span>
									{timestamp ? (
										<span
											className="shrink-0 text-[10px] text-muted-foreground"
											title={
												comment.createdAt
													? new Date(comment.createdAt).toLocaleString()
													: undefined
											}
										>
											{timestamp}
										</span>
									) : null}
								</div>
								<p className="mt-1 whitespace-pre-wrap break-words text-[12px] leading-[1.55] text-foreground/90">
									{comment.body}
								</p>
							</li>
						);
					})}
				</ol>

				{snippet.trim().length > 0 ? (
					<figure className="mt-3 overflow-hidden rounded-lg border border-border/60 bg-muted/30">
						<figcaption className="flex items-center gap-1.5 border-b border-border/50 px-2.5 py-1 text-[9.5px] font-medium uppercase tracking-wide text-muted-foreground">
							<span className="size-1 rounded-full bg-primary/60" aria-hidden />
							{t("reviewThread.codeContext")}
							{line ? (
								<span className="ml-auto font-mono normal-case tracking-normal text-foreground/70">
									L{line}
								</span>
							) : null}
						</figcaption>
						<pre className="max-h-36 overflow-auto px-2.5 py-2 font-mono text-[10.5px] leading-[1.5] text-foreground/80">
							{snippet}
						</pre>
					</figure>
				) : null}
			</div>

			<footer className="flex flex-wrap items-center gap-1.5 border-t border-border/60 bg-muted/20 px-3 py-2.5">
				{githubUrl ? (
					<Button
						type="button"
						variant="ghost"
						size="xs"
						className="mr-auto h-7 gap-1.5 px-2 text-muted-foreground hover:text-foreground"
						onClick={() => void openExternal(githubUrl)}
					>
						<ExternalLink className="size-3.5" strokeWidth={2} />
						{t("reviewThread.openOnGithub")}
					</Button>
				) : (
					<span className="mr-auto" />
				)}
				{onEditInComposer ? (
					<Button
						type="button"
						variant="outline"
						size="xs"
						className="h-7"
						onClick={onEditInComposer}
					>
						{t("reviewThread.composer")}
					</Button>
				) : null}
				<Button
					type="button"
					variant="default"
					size="xs"
					className="h-7 gap-1.5 shadow-sm"
					onClick={onSendToAgent}
				>
					<Send className="size-3.5" strokeWidth={2} />
					{t("reviewThread.sendToAgent")}
				</Button>
			</footer>
		</aside>
	);
}

export function WorkspaceEditorSurface({
	workspaceRoot,
	selection,
	onClose,
	onSubmitAnnotation,
	onEditInComposer,
	onAddToReview,
}: WorkspaceEditorSurfaceProps) {
	const { t } = useTranslation("common");
	const [pending, setPending] = useState<PendingAnnotation | null>(null);
	const [activeReviewThreadId, setActiveReviewThreadId] = useState<number | null>(
		null,
	);
	const annotationsEnabled = Boolean(
		onSubmitAnnotation || onEditInComposer || onAddToReview,
	);
	const reviewThreads = useMemo(
		() => buildReviewThreads(selection.reviewComments ?? []),
		[selection.reviewComments],
	);
	const reviewCommentAnnotations = useMemo(
		() => buildReviewCommentAnnotations(reviewThreads),
		[reviewThreads],
	);
	const machineAnnotations = useMemo(
		() => [...(selection.machineAnnotations ?? []), ...reviewCommentAnnotations],
		[reviewCommentAnnotations, selection.machineAnnotations],
	);
	const activeReviewThread = useMemo(
		() =>
			activeReviewThreadId == null
				? null
				: reviewThreads.find((thread) => thread.id === activeReviewThreadId) ?? null,
		[activeReviewThreadId, reviewThreads],
	);

	useEffect(() => {
		setActiveReviewThreadId(null);
	}, [selection.path, selection.group, selection.baseBranch]);

	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.defaultPrevented || event.key !== "Escape") {
				return;
			}
			if (shouldIgnoreGlobalShortcutTarget(event.target)) {
				return;
			}

			event.preventDefault();
			onClose();
		};

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [onClose]);

	const handleAnnotate = useCallback(
		(payload: DiffAnnotationPayload) => {
			const { anchor, ...rest } = payload;
			setPending({ request: { ...rest, path: selection.path }, anchor });
		},
		[selection.path],
	);

	const handleSubmitAnnotation = useCallback(
		(instruction: string, newSession: boolean) => {
			if (pending) {
				onSubmitAnnotation?.({
					request: pending.request,
					instruction,
					newSession,
				});
			}
			setPending(null);
			// Reveal the thread (the surface replaces it full-screen) so the
			// reviewer sees the agent pick up the request.
			onClose();
		},
		[onClose, onSubmitAnnotation, pending],
	);

	const handleEditInComposer = useCallback(
		(instruction: string) => {
			if (pending) {
				onEditInComposer?.({ request: pending.request, instruction });
			}
			setPending(null);
			onClose();
		},
		[onClose, onEditInComposer, pending],
	);

	const handleAddToReview = useCallback(
		(note: string) => {
			if (pending) {
				onAddToReview?.({ request: pending.request, note });
			}
			// Keep the surface open so the reviewer can collect more snippets.
			setPending(null);
		},
		[onAddToReview, pending],
	);

	const effectiveWorkspaceRoot = selection.workspaceRootOverride ?? workspaceRoot;
	const query = useWorkspaceGitFilePreviewContent(
		effectiveWorkspaceRoot
			? {
					workspaceRoot: effectiveWorkspaceRoot,
					relativePath: selection.path,
					status: selection.status,
					scope: selection.group,
					baseBranch: selection.baseBranch ?? null,
				}
			: null,
	);
	const loadErrorMessage =
		(query.error as Error | null)?.message ?? "Failed to load file";
	const snapshot = query.data ?? null;
	const activeReviewSnippet = useMemo(() => {
		if (!snapshot || !activeReviewThread) {
			return "";
		}
		const first = activeReviewThread.comments[0];
		if (!first) {
			return "";
		}
		const startLine = reviewCommentStartLine(first) ?? reviewCommentLine(first) ?? 1;
		const endLine = reviewCommentLine(first) ?? startLine;
		const sourceText =
			reviewCommentSide(first) === "original"
				? snapshot.originalText
				: snapshot.modifiedText;
		return snippetFromLines(sourceText, startLine, endLine);
	}, [activeReviewThread, snapshot]);

	const buildReviewAnnotationRequest = useCallback(
		(thread: ReviewThread): DiffAnnotationRequest => {
			const first = thread.comments[0]!;
			const startLine = reviewCommentStartLine(first) ?? reviewCommentLine(first) ?? 1;
			const endLine = reviewCommentLine(first) ?? startLine;
			return {
				path: selection.path,
				side: reviewCommentSide(first),
				startLine,
				endLine: Math.max(startLine, endLine),
				snippet: activeReviewSnippet,
			};
		},
		[activeReviewSnippet, selection.path],
	);

	const handleSendReviewThreadToAgent = useCallback(() => {
		if (!activeReviewThread) {
			return;
		}
		onSubmitAnnotation?.({
			request: buildReviewAnnotationRequest(activeReviewThread),
			instruction: buildReviewThreadPrompt({
				selection,
				thread: activeReviewThread,
				snippet: activeReviewSnippet,
			}),
			newSession: false,
		});
		setActiveReviewThreadId(null);
		onClose();
	}, [
		activeReviewSnippet,
		activeReviewThread,
		buildReviewAnnotationRequest,
		onClose,
		onSubmitAnnotation,
		selection,
	]);

	const handleEditReviewThreadInComposer = useCallback(() => {
		if (!activeReviewThread || !onEditInComposer) {
			return;
		}
		onEditInComposer({
			request: buildReviewAnnotationRequest(activeReviewThread),
			instruction: buildReviewThreadPrompt({
				selection,
				thread: activeReviewThread,
				snippet: activeReviewSnippet,
			}),
		});
		setActiveReviewThreadId(null);
		onClose();
	}, [
		activeReviewSnippet,
		activeReviewThread,
		buildReviewAnnotationRequest,
		onClose,
		onEditInComposer,
		selection,
	]);

	if (query.isPending) {
		return null;
	}

	if (query.isError) {
		return null;
	}

	return (
		<section
			aria-label="Workspace editor surface"
			data-focus-scope="editor"
			className="flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground"
		>
			<div
				className="flex h-9 items-center border-b border-border"
				data-tauri-drag-region
			>
				<TrafficLightSpacer side="left" width={86} />
				<div className="min-w-0 flex-1" data-tauri-drag-region />
				<div className="min-w-0 px-3 text-[11px] text-muted-foreground">
					{selection.name}
				</div>
				<div className="flex shrink-0 items-center pr-2">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={onClose}
						aria-label="Close editor view"
						className="gap-1.5 px-2 text-muted-foreground hover:text-foreground"
					>
						<ShortcutDisplay hotkey="Escape" />
						<X className="size-3.5" strokeWidth={1.8} />
					</Button>
				</div>
			</div>
			<div className="relative flex min-h-0 flex-1 bg-background">
				{query.isError ? (
					<div className="absolute inset-0 flex items-center justify-center bg-background">
						<p className="text-[11px] text-destructive">
							{loadErrorMessage}
						</p>
					</div>
				) : query.isPending || !snapshot ? (
					<div className="absolute inset-0 flex items-center justify-center bg-background">
						<p className="text-[11px] text-muted-foreground">Loading file...</p>
					</div>
				) : (
					<WorkspaceEditorDiff
						path={selection.path}
						originalText={snapshot.originalText}
						modifiedText={snapshot.modifiedText}
						inline={snapshot.inline}
						focusLine={selection.focusLine ?? null}
						machineAnnotations={machineAnnotations}
						onAnnotate={annotationsEnabled ? handleAnnotate : undefined}
						annotateLabel={t("diffAnnotate.sendToAgent")}
						onMachineAnnotationClick={({ annotation }) => {
							if (annotation.source === "github-review" && annotation.id) {
								setActiveReviewThreadId(Number(annotation.id));
							}
						}}
						reviewCommentLabel={t("diffAnnotate.reviewComment")}
					/>
				)}
				{activeReviewThread ? (
					<ReviewThreadDrawer
						selection={selection}
						thread={activeReviewThread}
						snippet={activeReviewSnippet}
						onClose={() => setActiveReviewThreadId(null)}
						onSendToAgent={handleSendReviewThreadToAgent}
						onEditInComposer={
							onEditInComposer ? handleEditReviewThreadInComposer : undefined
						}
					/>
				) : null}
			</div>
			{pending ? (
				<DiffAnnotationPopover
					pending={pending}
					canEditInComposer={Boolean(onEditInComposer)}
					canAddToReview={Boolean(onAddToReview)}
					onSubmit={handleSubmitAnnotation}
					onEditInComposer={handleEditInComposer}
					onAddToReview={handleAddToReview}
					onCancel={() => setPending(null)}
				/>
			) : null}
		</section>
	);
}
