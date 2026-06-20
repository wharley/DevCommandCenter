import { X } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { TrafficLightSpacer } from "@/components/chrome/traffic-light-spacer";
import { Button } from "@/components/ui/button";
import { useWorkspaceGitFilePreviewContent } from "@/features/inspector/use-workspace-git-file-preview-content";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";
import { ShortcutDisplay } from "@/features/shortcuts/shortcut-display";
import { shouldIgnoreGlobalShortcutTarget } from "@/features/shortcuts/shortcut-utils";
import type { DiffAnnotationPayload } from "@/lib/monaco-runtime";
import {
	DiffAnnotationPopover,
	type DiffAnnotationRequest,
	type DiffAnnotationSubmit,
	type PendingAnnotation,
} from "./diff-annotation";
import { FileViewToggle } from "./file-view-toggle";
import { useWorkspaceFileContent } from "./use-workspace-file-content";

/**
 * Where the file body comes from. `git` is the diff surface's whole-file toggle
 * (has a diff to return to); `path` is a standalone open (e.g. Quick Open).
 */
export type FileSurfaceSource =
	| { kind: "git"; selection: WorkspaceGitPreviewSelection }
	| {
			kind: "path";
			path: string;
			name: string;
			focusLine?: number | null;
	  };

type WorkspaceFileSurfaceProps = {
	workspaceRoot: string | null;
	source: FileSurfaceSource;
	/** Return to the diff view of the same file. Only set for the `git` source. */
	onBackToDiff?: () => void;
	onClose: () => void;
	onSubmitAnnotation?: (input: DiffAnnotationSubmit) => void;
	onEditInComposer?: (input: {
		request: DiffAnnotationRequest;
		instruction: string;
	}) => void;
	onAddToReview?: (input: {
		request: DiffAnnotationRequest;
		note: string;
	}) => void;
};

type MonacoRuntimeModule = typeof import("@/lib/monaco-runtime");
type MonacoFileController = Awaited<
	ReturnType<MonacoRuntimeModule["createFileEditor"]>
>;

function WorkspaceFileEditor({
	path,
	content,
	focusLine,
	onAnnotate,
	annotateLabel,
}: {
	path: string;
	content: string;
	focusLine?: number | null;
	onAnnotate?: (payload: DiffAnnotationPayload) => void;
	annotateLabel: string;
}) {
	const hostRef = useRef<HTMLDivElement | null>(null);
	const controllerRef = useRef<MonacoFileController | null>(null);
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

	// Recreate the editor only when the file identity changes. Content edits to the
	// same file are pushed through setValue below so selection state survives.
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
				const { createFileEditor } = await import("@/lib/monaco-runtime");
				const controller = await createFileEditor({
					container: host,
					path,
					content,
					readOnly: true,
					line: focusLine ?? undefined,
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
		// `content` is intentionally omitted: it flows through the setValue effect so
		// switching files (path) is the only trigger that rebuilds the editor.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [path]);

	useEffect(() => {
		controllerRef.current?.setValue(content);
	}, [content]);

	useEffect(() => {
		if (focusLine) {
			controllerRef.current?.revealPosition(focusLine);
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

export function WorkspaceFileSurface({
	workspaceRoot,
	source,
	onBackToDiff,
	onClose,
	onSubmitAnnotation,
	onEditInComposer,
	onAddToReview,
}: WorkspaceFileSurfaceProps) {
	const { t } = useTranslation("common");
	const [pending, setPending] = useState<PendingAnnotation | null>(null);
	const annotationsEnabled = Boolean(
		onSubmitAnnotation || onEditInComposer || onAddToReview,
	);

	const filePath = source.kind === "git" ? source.selection.path : source.path;
	const fileName = source.kind === "git" ? source.selection.name : source.name;
	const focusLine =
		source.kind === "git" ? source.selection.focusLine : source.focusLine;

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
			setPending({ request: { ...rest, path: filePath }, anchor });
		},
		[filePath],
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

	// Both hooks are always called (rules of hooks); only the one matching the
	// active source is enabled, the other is disabled with a null input.
	const gitQuery = useWorkspaceGitFilePreviewContent(
		workspaceRoot && source.kind === "git"
			? {
					workspaceRoot,
					relativePath: source.selection.path,
					status: source.selection.status,
					scope: source.selection.group,
					baseBranch: source.selection.baseBranch ?? null,
				}
			: null,
	);
	const pathQuery = useWorkspaceFileContent(
		workspaceRoot && source.kind === "path"
			? { workspaceRoot, relativePath: source.path }
			: null,
	);

	const query = source.kind === "git" ? gitQuery : pathQuery;
	// The whole-file body is the working-tree content. The diff preview exposes it
	// as the modified side (fall back to original for pure deletions); the path
	// reader returns it directly.
	const body =
		source.kind === "git"
			? gitQuery.data
				? gitQuery.data.modifiedText || gitQuery.data.originalText
				: ""
			: (pathQuery.data?.content ?? "");

	return (
		<section
			aria-label={t("fileSurface.ariaLabel")}
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
					{fileName}
				</div>
				{onBackToDiff ? (
					<div className="shrink-0 pr-2">
						<FileViewToggle mode="file" onSelectDiff={onBackToDiff} />
					</div>
				) : null}
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
							{(query.error as Error | null)?.message ?? "Failed to load file"}
						</p>
					</div>
				) : query.isPending ? (
					<div className="absolute inset-0 flex items-center justify-center bg-background">
						<p className="text-[11px] text-muted-foreground">
							{t("fileSurface.loading")}
						</p>
					</div>
				) : body.length === 0 ? (
					<div className="absolute inset-0 flex items-center justify-center bg-background">
						<p className="text-[11px] text-muted-foreground">
							{t("fileSurface.empty")}
						</p>
					</div>
				) : (
					<WorkspaceFileEditor
						path={filePath}
						content={body}
						focusLine={focusLine ?? null}
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
