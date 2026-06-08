import { X } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { TrafficLightSpacer } from "@/components/chrome/traffic-light-spacer";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { shouldIgnoreGlobalShortcutTarget } from "@/features/shortcuts/shortcut-utils";
import { ShortcutDisplay } from "@/features/shortcuts/shortcut-display";
import { useWorkspaceGitFilePreviewContent } from "@/features/inspector/use-workspace-git-file-preview-content";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";
import type { DiffAnnotationPayload } from "@/lib/monaco-runtime";

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
	onAnnotate,
	annotateLabel,
}: {
	path: string;
	originalText: string;
	modifiedText: string;
	inline: boolean;
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
	}, [inline, modifiedText, originalText, path]);

	useEffect(() => {
		controllerRef.current?.setTexts({
			originalText,
			modifiedText,
			inline,
		});
	}, [inline, modifiedText, originalText]);

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
	onSubmit,
	onEditInComposer,
	onCancel,
}: {
	pending: PendingAnnotation;
	canEditInComposer: boolean;
	onSubmit: (instruction: string, newSession: boolean) => void;
	onEditInComposer: (instruction: string) => void;
	onCancel: () => void;
}) {
	const { t } = useTranslation("common");
	const [instruction, setInstruction] = useState("");
	const textareaRef = useRef<HTMLTextAreaElement | null>(null);

	useEffect(() => {
		textareaRef.current?.focus();
	}, []);

	const { request, anchor } = pending;
	const lineLabel =
		request.startLine === request.endLine
			? `L${request.startLine}`
			: `L${request.startLine}–${request.endLine}`;
	const sideLabel =
		request.side === "original" ? t("diffAnnotate.deletedSide") : null;
	// Anchor above the trigger, unless it sits too close to the top edge.
	const placeBelow = anchor.top < 240;
	const trimmed = instruction.trim();

	return (
		<>
			<div
				className="fixed inset-0 z-40"
				onMouseDown={onCancel}
				aria-hidden
			/>
			<div
				role="dialog"
				aria-label={t("diffAnnotate.dialogLabel")}
				className="fixed z-50 w-[340px] max-w-[calc(100vw-2rem)] rounded-xl border border-border bg-popover p-3 text-popover-foreground shadow-lg"
				style={{
					top: anchor.top,
					left: Math.max(12, Math.min(anchor.left, window.innerWidth - 352)),
					transform: placeBelow
						? "translateY(24px)"
						: "translateY(calc(-100% - 8px))",
				}}
				onMouseDown={(event) => event.stopPropagation()}
			>
				<div className="mb-2 flex items-center gap-1.5 text-[11px] text-muted-foreground">
					<span className="truncate font-mono" title={request.path}>
						{request.path}
					</span>
					<span className="shrink-0 rounded bg-muted px-1 py-0.5 font-medium tabular-nums">
						{lineLabel}
					</span>
					{sideLabel ? (
						<span className="shrink-0 rounded bg-destructive/15 px-1 py-0.5 font-medium text-destructive">
							{sideLabel}
						</span>
					) : null}
				</div>
				<Textarea
					ref={textareaRef}
					value={instruction}
					onChange={(event) => setInstruction(event.target.value)}
					placeholder={t("diffAnnotate.instructionPlaceholder")}
					className="min-h-[72px] text-[13px]"
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
							trimmed.length > 0
						) {
							event.preventDefault();
							onSubmit(trimmed, false);
						}
					}}
				/>
				<div className="mt-2.5 flex items-center justify-between gap-2">
					{canEditInComposer ? (
						<Button
							type="button"
							variant="ghost"
							size="xs"
							className="text-muted-foreground"
							onClick={() => onEditInComposer(trimmed)}
						>
							{t("diffAnnotate.editInComposer")}
						</Button>
					) : (
						<span />
					)}
					<div className="flex items-center gap-1.5">
						<Button
							type="button"
							variant="outline"
							size="xs"
							disabled={trimmed.length === 0}
							onClick={() => onSubmit(trimmed, true)}
						>
							{t("diffAnnotate.newSession")}
						</Button>
						<Button
							type="button"
							variant="default"
							size="xs"
							disabled={trimmed.length === 0}
							onClick={() => onSubmit(trimmed, false)}
						>
							{t("diffAnnotate.send")}
						</Button>
					</div>
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
}: WorkspaceEditorSurfaceProps) {
	const { t } = useTranslation("common");
	const [pending, setPending] = useState<PendingAnnotation | null>(null);
	const annotationsEnabled = Boolean(onSubmitAnnotation || onEditInComposer);
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
						onAnnotate={annotationsEnabled ? handleAnnotate : undefined}
						annotateLabel={t("diffAnnotate.sendToAgent")}
					/>
				)}
			</div>
			{pending ? (
				<DiffAnnotationPopover
					pending={pending}
					canEditInComposer={Boolean(onEditInComposer)}
					onSubmit={handleSubmitAnnotation}
					onEditInComposer={handleEditInComposer}
					onCancel={() => setPending(null)}
				/>
			) : null}
		</section>
	);
}
