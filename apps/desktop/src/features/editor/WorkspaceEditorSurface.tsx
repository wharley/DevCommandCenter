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
