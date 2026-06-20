import { X } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { TrafficLightSpacer } from "@/components/chrome/traffic-light-spacer";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { shouldIgnoreGlobalShortcutTarget } from "@/features/shortcuts/shortcut-utils";
import { ShortcutDisplay } from "@/features/shortcuts/shortcut-display";
import { InlineShortcutDisplay } from "@/features/shortcuts/InlineShortcutDisplay";
import { useWorkspaceGitFilePreviewContent } from "@/features/inspector/use-workspace-git-file-preview-content";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";
import type {
	DiffAnnotationPayload,
	DiffMachineAnnotation,
} from "@/lib/monaco-runtime";

/** A diff annotation bound to the file it was selected in (anchor stripped). */
export type DiffAnnotationRequest = Omit<DiffAnnotationPayload, "anchor"> & {
	path: string;
};

export type DiffAnnotationSubmit = {
	request: DiffAnnotationRequest;
	instruction: string;
	newSession: boolean;
};

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

type PendingAnnotation = {
	request: DiffAnnotationRequest;
	anchor: { top: number; left: number };
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
}: {
	path: string;
	originalText: string;
	modifiedText: string;
	inline: boolean;
	focusLine?: number | null;
	machineAnnotations?: DiffMachineAnnotation[];
	onAnnotate?: (payload: DiffAnnotationPayload) => void;
	annotateLabel: string;
}) {
	const hostRef = useRef<HTMLDivElement | null>(null);
	const controllerRef = useRef<MonacoDiffController | null>(null);
	const requestIdRef = useRef(0);
	// Keep the latest callback/label in refs so they never recreate the editor.
	const onAnnotateRef = useRef(onAnnotate);
	onAnnotateRef.current = onAnnotate;
	const annotateLabelRef = useRef(annotateLabel);
	annotateLabelRef.current = annotateLabel;
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
				});

				if (disposed || requestId !== requestIdRef.current) {
					controller.dispose();
					return;
				}

				controllerRef.current = controller;
				setLoading(false);
			} catch (cause) {
				if (disposed) return;
				setError(cause instanceof Error ? cause.message : "Failed to load editor");
				setLoading(false);
			}
		})();

		return () => {
			disposed = true;
		};
	}, [focusLine, inline, machineAnnotations, modifiedText, originalText, path]);

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

function DiffAnnotationPopover({
	pending,
	canEditInComposer,
	canAddToReview,
	onSubmit,
	onEditInComposer,
	onAddToReview,
	onCancel,
}: {
	pending: PendingAnnotation;
	canEditInComposer: boolean;
	canAddToReview: boolean;
	onSubmit: (instruction: string, newSession: boolean) => void;
	onEditInComposer: (instruction: string) => void;
	onAddToReview: (note: string) => void;
	onCancel: () => void;
}) {
	const { t } = useTranslation("common");
	const [instruction, setInstruction] = useState("");
	const textareaRef = useRef<HTMLTextAreaElement | null>(null);
	const cardRef = useRef<HTMLDivElement | null>(null);
	// Resolved viewport position. Kept null until measured so the card never
	// flashes at an unclamped spot (which is what pushed the actions off-screen).
	const [position, setPosition] = useState<{ top: number; left: number } | null>(
		null,
	);

	useEffect(() => {
		textareaRef.current?.focus();
	}, []);

	const { request, anchor } = pending;

	// Measure the rendered card and place it fully inside the viewport: prefer
	// above the trigger, fall back to below, then clamp on every edge. This
	// guarantees the footer (and the primary "Send" action) is always visible.
	useLayoutEffect(() => {
		const card = cardRef.current;
		if (!card) {
			return;
		}

		const measure = () => {
			const margin = 12;
			const gap = 8;
			const { offsetWidth: width, offsetHeight: height } = card;
			const viewportW = window.innerWidth;
			const viewportH = window.innerHeight;

			let left = Math.min(anchor.left, viewportW - width - margin);
			left = Math.max(margin, left);

			const above = anchor.top - gap - height;
			const below = anchor.top + 24;
			let top = above >= margin ? above : below;
			top = Math.min(top, viewportH - height - margin);
			top = Math.max(margin, top);

			setPosition({ top, left });
		};

		measure();
		window.addEventListener("resize", measure);
		return () => window.removeEventListener("resize", measure);
	}, [anchor.left, anchor.top]);

	const lineLabel =
		request.startLine === request.endLine
			? `L${request.startLine}`
			: `L${request.startLine}–${request.endLine}`;
	const sideLabel =
		request.side === "original" ? t("diffAnnotate.deletedSide") : null;
	const trimmed = instruction.trim();
	const canSubmit = trimmed.length > 0;

	return (
		<>
			<div
				className="fixed inset-0 z-40 bg-black/5 supports-backdrop-filter:backdrop-blur-[1px] animate-in fade-in-0 duration-100"
				onMouseDown={onCancel}
				aria-hidden
			/>
			<div
				ref={cardRef}
				role="dialog"
				aria-label={t("diffAnnotate.dialogLabel")}
				className="fixed z-50 flex max-h-[calc(100vh-1.5rem)] w-[348px] max-w-[calc(100vw-1.5rem)] origin-top flex-col overflow-hidden rounded-xl border border-border/80 bg-popover text-popover-foreground shadow-xl ring-1 ring-foreground/10 animate-in fade-in-0 zoom-in-95 duration-100"
				style={{
					top: position?.top ?? anchor.top,
					left: position?.left ?? anchor.left,
					visibility: position ? "visible" : "hidden",
				}}
				onMouseDown={(event) => event.stopPropagation()}
			>
				<div className="flex items-center gap-1.5 border-b border-border/60 bg-muted/30 px-3 py-2 text-[11px]">
					<span className="size-1.5 shrink-0 rounded-full bg-primary/70" aria-hidden />
					<span
						className="min-w-0 flex-1 truncate font-mono text-muted-foreground"
						title={request.path}
						dir="rtl"
					>
						{request.path}
					</span>
					<span className="shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono font-medium tabular-nums text-foreground/80">
						{lineLabel}
					</span>
					{sideLabel ? (
						<span className="shrink-0 rounded bg-destructive/15 px-1.5 py-0.5 font-medium text-destructive">
							{sideLabel}
						</span>
					) : null}
				</div>

				<div className="flex min-h-0 flex-col gap-2.5 overflow-y-auto p-3">
					<Textarea
						ref={textareaRef}
						value={instruction}
						onChange={(event) => setInstruction(event.target.value)}
						placeholder={t("diffAnnotate.instructionPlaceholder")}
						className="min-h-[76px] resize-none text-[13px] leading-relaxed"
						onKeyDown={(event) => {
							if (event.key === "Escape") {
								event.preventDefault();
								event.stopPropagation();
								onCancel();
								return;
							}
							if (
								(event.metaKey || event.ctrlKey) &&
								event.key === "Enter" &&
								canSubmit
							) {
								event.preventDefault();
								onSubmit(trimmed, false);
							}
						}}
					/>

					<div className="flex items-center justify-between gap-2 text-[10.5px] text-muted-foreground">
						<span className="inline-flex items-center gap-1">
							<InlineShortcutDisplay keys={["⌘", "↵"]} />
							{t("diffAnnotate.send")}
						</span>
						{canAddToReview ? (
							<Button
								type="button"
								variant="ghost"
								size="xs"
								className="h-6 px-1.5 text-muted-foreground hover:text-foreground"
								disabled={!canSubmit}
								onClick={() => onAddToReview(trimmed)}
							>
								{t("diffAnnotate.addToReview")}
							</Button>
						) : null}
					</div>
				</div>

				<div className="flex flex-wrap items-center justify-end gap-1.5 border-t border-border/60 bg-muted/20 px-3 py-2.5">
					{canEditInComposer ? (
						<Button
							type="button"
							variant="ghost"
							size="xs"
							className="mr-auto h-7 px-2 text-muted-foreground hover:text-foreground"
							onClick={() => onEditInComposer(trimmed)}
						>
							{t("diffAnnotate.editInComposer")}
						</Button>
					) : null}
					<Button
						type="button"
						variant="outline"
						size="xs"
						className="h-7"
						disabled={!canSubmit}
						onClick={() => onSubmit(trimmed, true)}
					>
						{t("diffAnnotate.newSession")}
					</Button>
					<Button
						type="button"
						variant="default"
						size="xs"
						className="h-7 shadow-sm"
						disabled={!canSubmit}
						onClick={() => onSubmit(trimmed, false)}
					>
						{t("diffAnnotate.send")}
					</Button>
				</div>
			</div>
		</>
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
	const annotationsEnabled = Boolean(
		onSubmitAnnotation || onEditInComposer || onAddToReview,
	);
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

	const query = useWorkspaceGitFilePreviewContent(
		workspaceRoot
			? {
					workspaceRoot,
					relativePath: selection.path,
					status: selection.status,
					scope: selection.group,
					baseBranch: selection.baseBranch ?? null,
				}
			: null,
	);
	const loadErrorMessage =
		(query.error as Error | null)?.message ?? "Failed to load file";

	if (query.isPending) {
		return null;
	}

	if (query.isError) {
		return null;
	}

	const snapshot = query.data;

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
						machineAnnotations={selection.machineAnnotations}
						onAnnotate={annotationsEnabled ? handleAnnotate : undefined}
						annotateLabel={t("diffAnnotate.sendToAgent")}
					/>
				)}
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
