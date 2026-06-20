import { Loader2, Save, X } from "lucide-react";
import {
	forwardRef,
	useCallback,
	useEffect,
	useImperativeHandle,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { TrafficLightSpacer } from "@/components/chrome/traffic-light-spacer";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { writeWorkspaceFile } from "@/lib/workspace-api";
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
import { resolveFileSurfaceContentState } from "./file-surface.logic";
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

/** Imperative handle the tab wrapper uses to drive the active surface. */
export type WorkspaceFileSurfaceHandle = {
	save: () => void;
	/** Re-measure + focus the editor when this surface's tab becomes active. */
	reveal: () => void;
};

type WorkspaceFileSurfaceProps = {
	workspaceRoot: string | null;
	source: FileSurfaceSource;
	/** Allow editing + saving. Off for the `git` review toggle. */
	editable?: boolean;
	/**
	 * Render body-only (no header, no own key bindings) so a parent — the tab
	 * wrapper — can provide shared chrome and keep multiple surfaces alive.
	 */
	embedded?: boolean;
	/** Report dirty/saving up so the tab strip can show state and pin previews. */
	onStateChange?: (state: { dirty: boolean; saving: boolean }) => void;
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

export type FileEditorHandle = {
	getValue: () => string;
	setValue: (value: string) => void;
	/** Re-measure after the host becomes visible again (kept-alive tab switch). */
	layout: () => void;
	focus: () => void;
};

const WorkspaceFileEditor = forwardRef<
	FileEditorHandle,
	{
		path: string;
		content: string;
		focusLine?: number | null;
		readOnly: boolean;
		onAnnotate?: (payload: DiffAnnotationPayload) => void;
		annotateLabel: string;
		onChange?: () => void;
	}
>(function WorkspaceFileEditor(
	{ path, content, focusLine, readOnly, onAnnotate, annotateLabel, onChange },
	ref,
) {
	const hostRef = useRef<HTMLDivElement | null>(null);
	const controllerRef = useRef<MonacoFileController | null>(null);
	const requestIdRef = useRef(0);
	// Keep the latest callbacks/label in refs so they never recreate the editor.
	const onAnnotateRef = useRef(onAnnotate);
	onAnnotateRef.current = onAnnotate;
	const onChangeRef = useRef(onChange);
	onChangeRef.current = onChange;
	const annotateLabelRef = useRef(annotateLabel);
	annotateLabelRef.current = annotateLabel;
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);

	useImperativeHandle(
		ref,
		() => ({
			getValue: () => controllerRef.current?.getValue() ?? "",
			setValue: (value: string) => controllerRef.current?.setValue(value),
			layout: () => controllerRef.current?.editor.layout(),
			focus: () => controllerRef.current?.editor.focus(),
		}),
		[],
	);

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
		let changeSub: { dispose(): void } | null = null;

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
					readOnly,
					line: focusLine ?? undefined,
					onAnnotate: (payload) => onAnnotateRef.current?.(payload),
					annotateLabel: annotateLabelRef.current,
				});

				if (disposed || requestId !== requestIdRef.current) {
					controller.dispose();
					return;
				}

				controllerRef.current = controller;
				changeSub = controller.onDidChangeModelContent(() =>
					onChangeRef.current?.(),
				);
				setLoading(false);
			} catch (cause) {
				if (disposed) return;
				setError(cause instanceof Error ? cause.message : "Failed to load editor");
				setLoading(false);
			}
		})();

		return () => {
			disposed = true;
			changeSub?.dispose();
		};
		// `content`/`readOnly` are intentionally omitted: content flows through the
		// setValue effect and readOnly is fixed per surface, so switching files (path)
		// is the only trigger that rebuilds the editor.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [path]);

	// Only the read-only surface mirrors later content into the editor. In edit
	// mode the buffer is user-owned (synced via the imperative handle), so we never
	// push query updates back in — that would clobber unsaved edits.
	useEffect(() => {
		if (readOnly) {
			controllerRef.current?.setValue(content);
		}
	}, [content, readOnly]);

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
});

export const WorkspaceFileSurface = forwardRef<
	WorkspaceFileSurfaceHandle,
	WorkspaceFileSurfaceProps
>(function WorkspaceFileSurface(
	{
		workspaceRoot,
		source,
		editable,
		embedded,
		onStateChange,
		onBackToDiff,
		onClose,
		onSubmitAnnotation,
		onEditInComposer,
		onAddToReview,
	},
	ref,
) {
	const { t } = useTranslation("common");
	const [pending, setPending] = useState<PendingAnnotation | null>(null);
	const annotationsEnabled = Boolean(
		onSubmitAnnotation || onEditInComposer || onAddToReview,
	);

	const filePath = source.kind === "git" ? source.selection.path : source.path;
	const fileName = source.kind === "git" ? source.selection.name : source.name;
	const focusLine =
		source.kind === "git" ? source.selection.focusLine : source.focusLine;
	const canEdit = Boolean(editable && workspaceRoot);

	const editorRef = useRef<FileEditorHandle | null>(null);
	// Disk content as of open / last save / last reload. Reconciliation compares
	// against this to detect a concurrent change (e.g. an agent on the same CWD).
	const baseContentRef = useRef("");
	const [dirty, setDirty] = useState(false);
	const [saving, setSaving] = useState(false);
	// When set, the file diverged on disk at save time; the dialog forces a choice.
	const [reconcile, setReconcile] = useState<{
		disk: string;
		mine: string;
	} | null>(null);

	const handleEditorChange = useCallback(() => {
		const value = editorRef.current?.getValue() ?? "";
		setDirty(value !== baseContentRef.current);
	}, []);

	const handleEditorSaved = useCallback(
		(content: string) => {
			baseContentRef.current = content;
			setDirty(false);
			toast.success(t("fileSurface.saved", { name: fileName }));
		},
		[fileName, t],
	);

	const handleSave = useCallback(async () => {
		if (!canEdit || !workspaceRoot || saving) return;
		const mine = editorRef.current?.getValue() ?? "";
		if (mine === baseContentRef.current) {
			setDirty(false);
			return;
		}
		setSaving(true);
		try {
			// Compare-and-swap on the backend: it re-reads + writes atomically and only
			// overwrites if the disk still equals what we last saw. A mismatch (likely
			// an agent on the same CWD) comes back as `conflicted` for reconciliation.
			const result = await writeWorkspaceFile({
				workspaceRoot,
				relativePath: filePath,
				content: mine,
				expectedPrevious: baseContentRef.current,
			});
			if (result.conflicted) {
				setReconcile({ disk: result.diskContent ?? "", mine });
				return;
			}
			handleEditorSaved(mine);
		} catch (cause) {
			toast.error(
				cause instanceof Error ? cause.message : t("fileSurface.saveFailed"),
			);
		} finally {
			setSaving(false);
		}
	}, [canEdit, filePath, handleEditorSaved, saving, t, workspaceRoot]);

	// Reconciliation choices.
	const handleOverwrite = useCallback(async () => {
		if (!reconcile || !workspaceRoot) return;
		setSaving(true);
		try {
			// Force the write past the guard — the user explicitly chose to overwrite.
			await writeWorkspaceFile({
				workspaceRoot,
				relativePath: filePath,
				content: reconcile.mine,
				expectedPrevious: null,
			});
			handleEditorSaved(reconcile.mine);
			setReconcile(null);
		} catch (cause) {
			toast.error(
				cause instanceof Error ? cause.message : t("fileSurface.saveFailed"),
			);
		} finally {
			setSaving(false);
		}
	}, [filePath, handleEditorSaved, reconcile, t, workspaceRoot]);

	const handleTakeDisk = useCallback(() => {
		if (!reconcile) return;
		editorRef.current?.setValue(reconcile.disk);
		baseContentRef.current = reconcile.disk;
		setDirty(false);
		setReconcile(null);
	}, [reconcile]);

	// Let the tab wrapper drive Save on the active surface and watch its state.
	useImperativeHandle(
		ref,
		() => ({
			save: () => void handleSave(),
			reveal: () => {
				editorRef.current?.layout();
				editorRef.current?.focus();
			},
		}),
		[handleSave],
	);

	const onStateChangeRef = useRef(onStateChange);
	onStateChangeRef.current = onStateChange;
	useEffect(() => {
		onStateChangeRef.current?.({ dirty, saving });
	}, [dirty, saving]);

	// Guard against losing unsaved edits when leaving the surface.
	const requestClose = useCallback(() => {
		if (dirty && !window.confirm(t("fileSurface.discardConfirm"))) {
			return;
		}
		onClose();
	}, [dirty, onClose, t]);

	useEffect(() => {
		// Embedded surfaces let the tab wrapper own global keys (one listener for
		// the active tab) instead of every kept-alive surface reacting at once.
		if (embedded) {
			return;
		}
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.defaultPrevented) {
				return;
			}
			// Save (Cmd/Ctrl+S) must fire even with focus inside the editor, so it
			// runs before the input guard below — a modifier chord is never text.
			if (
				canEdit &&
				(event.metaKey || event.ctrlKey) &&
				!event.shiftKey &&
				!event.altKey &&
				event.key.toLowerCase() === "s"
			) {
				event.preventDefault();
				void handleSave();
				return;
			}
			if (event.key !== "Escape") {
				return;
			}
			if (shouldIgnoreGlobalShortcutTarget(event.target)) {
				return;
			}

			event.preventDefault();
			requestClose();
		};

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [embedded, canEdit, handleSave, requestClose]);

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
	const contentState = resolveFileSurfaceContentState(query);

	// The loaded disk body becomes the reconciliation baseline. Fires only when the
	// content value actually changes (new file, or a refetch with new content).
	useEffect(() => {
		baseContentRef.current = body;
		setDirty(false);
	}, [body]);

	return (
		<section
			aria-label={t("fileSurface.ariaLabel")}
			data-focus-scope="editor"
			className="flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground"
		>
			{embedded ? null : (
			<div
				className="flex h-9 items-center border-b border-border"
				data-tauri-drag-region
			>
				<TrafficLightSpacer side="left" width={86} />
				<div className="min-w-0 flex-1" data-tauri-drag-region />
				<div className="flex min-w-0 items-center gap-1.5 px-3 text-[11px] text-muted-foreground">
					{canEdit && dirty ? (
						<span
							className="size-1.5 shrink-0 rounded-full bg-primary"
							aria-label={t("fileSurface.unsaved")}
						/>
					) : null}
					<span className="truncate">{fileName}</span>
				</div>
				{onBackToDiff ? (
					<div className="shrink-0 pr-2">
						<FileViewToggle mode="file" onSelectDiff={onBackToDiff} />
					</div>
				) : null}
				<div className="flex shrink-0 items-center gap-1 pr-2">
					{canEdit ? (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => void handleSave()}
							disabled={!dirty || saving}
							aria-label={t("fileSurface.save")}
							className="gap-1.5 px-2 text-muted-foreground hover:text-foreground"
						>
							{saving ? (
								<Loader2 className="size-3.5 animate-spin" strokeWidth={1.8} />
							) : (
								<Save className="size-3.5" strokeWidth={1.8} />
							)}
							<span>{t("fileSurface.save")}</span>
						</Button>
					) : null}
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={requestClose}
						aria-label="Close editor view"
						className="gap-1.5 px-2 text-muted-foreground hover:text-foreground"
					>
						<ShortcutDisplay hotkey="Escape" />
						<X className="size-3.5" strokeWidth={1.8} />
					</Button>
				</div>
			</div>
			)}
			<div className="relative flex min-h-0 flex-1 bg-background">
				{contentState === "error" ? (
					<div className="absolute inset-0 flex items-center justify-center bg-background">
						<p className="text-[11px] text-destructive">
							{(query.error as Error | null)?.message ?? "Failed to load file"}
						</p>
					</div>
				) : contentState === "loading" ? (
					<div className="absolute inset-0 flex items-center justify-center bg-background">
						<p className="text-[11px] text-muted-foreground">
							{t("fileSurface.loading")}
						</p>
					</div>
				) : (
					<WorkspaceFileEditor
						ref={editorRef}
						path={filePath}
						content={body}
						focusLine={focusLine ?? null}
						readOnly={!canEdit}
						onChange={canEdit ? handleEditorChange : undefined}
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
			<ReconcileDialog
				open={Boolean(reconcile)}
				fileName={fileName}
				path={filePath}
				disk={reconcile?.disk ?? ""}
				mine={reconcile?.mine ?? ""}
				saving={saving}
				onOverwrite={() => void handleOverwrite()}
				onTakeDisk={handleTakeDisk}
				onCancel={() => setReconcile(null)}
			/>
		</section>
	);
});

type MonacoDiffController = Awaited<
	ReturnType<MonacoRuntimeModule["createDiffEditor"]>
>;

/** Read-only diff of the on-disk version (left) against the user's edit (right). */
function ReconcileDiffHost({
	path,
	disk,
	mine,
}: {
	path: string;
	disk: string;
	mine: string;
}) {
	const hostRef = useRef<HTMLDivElement | null>(null);
	const controllerRef = useRef<MonacoDiffController | null>(null);

	useLayoutEffect(() => {
		const host = hostRef.current;
		if (!host) return;
		let disposed = false;

		host.replaceChildren();
		void (async () => {
			const { createDiffEditor } = await import("@/lib/monaco-runtime");
			const controller = await createDiffEditor({
				container: host,
				path,
				originalText: disk,
				modifiedText: mine,
				inline: false,
			});
			if (disposed) {
				controller.dispose();
				return;
			}
			controllerRef.current = controller;
		})();

		return () => {
			disposed = true;
			controllerRef.current?.dispose();
			controllerRef.current = null;
		};
	}, [path, disk, mine]);

	return <div ref={hostRef} className="h-full min-h-0 w-full" />;
}

function ReconcileDialog({
	open,
	fileName,
	path,
	disk,
	mine,
	saving,
	onOverwrite,
	onTakeDisk,
	onCancel,
}: {
	open: boolean;
	fileName: string;
	path: string;
	disk: string;
	mine: string;
	saving: boolean;
	onOverwrite: () => void;
	onTakeDisk: () => void;
	onCancel: () => void;
}) {
	const { t } = useTranslation("common");

	return (
		<Dialog open={open} onOpenChange={(next) => (next ? null : onCancel())}>
			<DialogContent
				showCloseButton={false}
				className="flex max-h-[min(80vh,720px)] w-[min(92vw,920px)] flex-col gap-0 overflow-hidden p-0"
			>
				<DialogHeader className="space-y-1 border-b border-border/60 px-4 py-3">
					<DialogTitle className="text-sm">
						{t("fileSurface.reconcile.title")}
					</DialogTitle>
					<DialogDescription className="text-[12px] leading-relaxed">
						{t("fileSurface.reconcile.description", { name: fileName })}
					</DialogDescription>
				</DialogHeader>
				<div className="flex items-center gap-3 px-4 py-1.5 text-[10.5px] font-medium uppercase tracking-[0.06em] text-muted-foreground">
					<span className="flex-1">{t("fileSurface.reconcile.diskLabel")}</span>
					<span className="flex-1">{t("fileSurface.reconcile.mineLabel")}</span>
				</div>
				<div className="min-h-0 flex-1 border-y border-border/60">
					{open ? <ReconcileDiffHost path={path} disk={disk} mine={mine} /> : null}
				</div>
				<DialogFooter className="gap-1.5 px-4 py-3">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={onCancel}
						disabled={saving}
						className="mr-auto"
					>
						{t("fileSurface.reconcile.cancel")}
					</Button>
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={onTakeDisk}
						disabled={saving}
					>
						{t("fileSurface.reconcile.takeDisk")}
					</Button>
					<Button
						type="button"
						variant="default"
						size="sm"
						onClick={onOverwrite}
						disabled={saving}
					>
						{t("fileSurface.reconcile.overwrite")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
