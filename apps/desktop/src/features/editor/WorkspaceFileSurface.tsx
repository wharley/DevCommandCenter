import type { SelectedLineRange } from "@pierre/diffs";
import { Editor, type EditorOptions } from "@pierre/diffs/edit";
import { EditProvider, File, Virtualizer, useVirtualizer } from "@pierre/diffs/react";
import { Loader2, MessageSquare, Save, X } from "lucide-react";
import {
	forwardRef,
	useCallback,
	useEffect,
	useImperativeHandle,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
	type RefObject,
	type ComponentProps,
	type MutableRefObject,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { TrafficLightSpacer } from "@/components/chrome/traffic-light-spacer";
import { Button } from "@/components/ui/button";
import { useAppearance } from "@/components/theme-provider";
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
import type { DiffAnnotationPayload } from "./diff-types";
import {
	DiffAnnotationPopover,
	type DiffAnnotationRequest,
	type DiffAnnotationSubmit,
	type PendingAnnotation,
} from "./diff-annotation";
import { resolveFileSurfaceContentState } from "./file-surface.logic";
import { useWorkspaceFileContent } from "./use-workspace-file-content";
import { WorkspaceChangesDiffLoader } from "./WorkspaceChangesDiffLoader";
import {
	collapsedEditorState,
	fullDocumentReplacement,
	primaryHandlePosition,
	snippetForOneBasedLines,
} from "./workspace-file-editor.logic";

/**
 * Where the file body comes from. `git` loads working-tree content from the
 * diff preview API; `path` is a standalone open (e.g. Quick Open or Code dock).
 */
export type FileSurfaceSource =
	| { kind: "git"; selection: WorkspaceGitPreviewSelection }
	| {
			kind: "path";
			path: string;
			name: string;
			focusLine?: number | null;
			focusColumn?: number | null;
	  };

/** Imperative handle the tab wrapper uses to drive the active surface. */
export type WorkspaceFileSurfaceHandle = {
	save: () => void;
	/** Re-measure + focus the editor when this surface's tab becomes active. */
	reveal: () => void;
	/** Notify dirty state after the shared editor buffer changes. */
	syncEditorChange: () => void;
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
	/** Restored buffer when remounting a tab the user already edited. */
	initialBuffer?: string | null;
	/** Persist in-flight edits when the tab surface unmounts. */
	onBufferSnapshot?: (content: string) => void;
	/** Tab strip renders one shared editor; this surface only loads content. */
	useSharedEditor?: boolean;
	/** Live editor handle for the active tab (shared editor). */
	editorBridge?: RefObject<FileEditorHandle | null>;
	/** Fired when disk content is ready for the shared editor. */
	onContentReady?: (content: string) => void;
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

export type FileEditorHandle = {
	getValue: () => string;
	setValue: (value: string) => void;
	getPath: () => string;
	getPosition: () => { lineNumber: number; column: number } | null;
	switchFile: (
		path: string,
		content: string,
		focusLine?: number | null,
		focusColumn?: number | null,
	) => boolean;
	/** Re-measure after the host becomes visible again (kept-alive tab switch). */
	layout: () => void;
	focus: () => void;
};

type PierreVirtualizer = ReturnType<typeof useVirtualizer>;
type SelectionActionContext = Parameters<
	NonNullable<EditorOptions<undefined>["renderSelectionAction"]>
>[0];

function triggerAnchor(target: HTMLElement | null): { top: number; left: number } {
	const rect = target?.getBoundingClientRect();
	return {
		top: rect?.bottom ?? Math.round(window.innerHeight / 2),
		left: rect?.left ?? Math.round(window.innerWidth / 2),
	};
}

function WorkspaceFileRenderer({
	file,
	readOnly,
	options,
	virtualizerRef,
	selectedLines,
}: {
	file: { name: string; contents: string; cacheKey: string };
	readOnly: boolean;
	options: ComponentProps<typeof File>["options"];
	virtualizerRef: MutableRefObject<PierreVirtualizer>;
	selectedLines?: SelectedLineRange | null;
}) {
	const virtualizer = useVirtualizer();
	useLayoutEffect(() => {
		virtualizerRef.current = virtualizer;
		return () => {
			if (virtualizerRef.current === virtualizer) virtualizerRef.current = undefined;
		};
	}, [virtualizer, virtualizerRef]);

	return (
		<File
			file={file}
			edit={!readOnly}
			disableWorkerPool
			options={options}
			selectedLines={selectedLines}
			className="block min-h-full min-w-full"
		/>
	);
}

export const WorkspaceFileEditor = forwardRef<
	FileEditorHandle,
	{
		path: string;
		content: string;
		focusLine?: number | null;
		cursorLine?: number | null;
		cursorColumn?: number | null;
		readOnly: boolean;
		/** Swap files on tab change instead of tearing down the editor (tab strip). */
		reuseInstance?: boolean;
		onAnnotate?: (payload: DiffAnnotationPayload) => void;
		annotateLabel: string;
		onChange?: () => void;
	}
>(function WorkspaceFileEditor(
	{
		path,
		content,
		focusLine,
		cursorLine,
		cursorColumn,
		readOnly,
		reuseInstance = false,
		onAnnotate,
		annotateLabel,
		onChange,
	},
	ref,
) {
	const { theme } = useAppearance();
	const hostRef = useRef<HTMLDivElement | null>(null);
	const virtualizerRef = useRef<PierreVirtualizer>(undefined);
	const editorRef = useRef<Editor<undefined> | null>(null);
	const [activeFile, setActiveFile] = useState(() => ({ path, content }));
	const [selectedLines, setSelectedLines] = useState<SelectedLineRange | null>(null);
	const valueRef = useRef(content);
	const pathRef = useRef(path);
	const positionRef = useRef<{ lineNumber: number; column: number } | null>(
		cursorLine != null
			? { lineNumber: cursorLine, column: cursorColumn ?? 1 }
			: focusLine != null
				? { lineNumber: focusLine, column: 1 }
				: null,
	);
	const pendingPositionRef = useRef(positionRef.current);
	const onAnnotateRef = useRef(onAnnotate);
	onAnnotateRef.current = onAnnotate;
	const onChangeRef = useRef(onChange);
	onChangeRef.current = onChange;
	const annotateLabelRef = useRef(annotateLabel);
	annotateLabelRef.current = annotateLabel;

	const emitAnnotation = useCallback(
		(startLine: number, endLine: number, snippet: string, target: HTMLElement | null) => {
			onAnnotateRef.current?.({
				side: "modified",
				startLine,
				endLine,
				snippet,
				anchor: triggerAnchor(target ?? hostRef.current),
			});
		},
		[],
	);
	const annotateSelectedLines = useCallback(
		(range: SelectedLineRange, target: HTMLElement | null) => {
			const startLine = Math.max(1, Math.min(range.start, range.end));
			const endLine = Math.max(startLine, Math.max(range.start, range.end));
			emitAnnotation(
				startLine,
				endLine,
				snippetForOneBasedLines(valueRef.current, startLine, endLine),
				target,
			);
			setSelectedLines(null);
		},
		[emitAnnotation],
	);

	const renderSelectionAction = useCallback(
		(context: SelectionActionContext) => {
			const button = document.createElement("button");
			button.type = "button";
			button.textContent = annotateLabelRef.current;
			button.className =
				"rounded bg-primary px-2 py-1 text-[11px] text-primary-foreground shadow-sm";
			button.addEventListener("click", () => {
				const startLine = Math.min(
					context.selection.start.line,
					context.selection.end.line,
				) + 1;
				const endLine = Math.max(
					context.selection.start.line,
					context.selection.end.line,
				) + 1;
				emitAnnotation(startLine, endLine, context.getSelectionText(), button);
				context.close();
			});
			return button;
		},
		[emitAnnotation],
	);

	if (!editorRef.current) {
		editorRef.current = new Editor<undefined>({
			historyMaxEntries: 32,
			persistState: false,
			roundedSelection: true,
			matchBrackets: true,
			enabledSelectionAction: Boolean(onAnnotate),
			renderSelectionAction,
			onAttach: (editor) => {
				const target = pendingPositionRef.current;
				if (target) editor.setState(collapsedEditorState(target.lineNumber, target.column));
			},
			onChange: (_file, _annotations, _event) => {
				const editor = editorRef.current;
				if (!editor) return;
				valueRef.current = editor.getText();
				positionRef.current = primaryHandlePosition(editor.getState());
				onChangeRef.current?.();
			},
		});
	}

	const createEditor = useCallback((options: EditorOptions<undefined>) => {
		const editor = editorRef.current;
		if (!editor) throw new Error("Workspace file editor was disposed");
		editor.setOptions({ ...options, persistState: false, historyMaxEntries: 32 });
		return editor;
	}, []);

	useImperativeHandle(
		ref,
		() => ({
			getValue: () => valueRef.current,
			setValue: (value: string) => {
				const editor = editorRef.current;
				const attachedFile = editor?.getFile();
				const replacement = editor && attachedFile?.name === pathRef.current
					? fullDocumentReplacement(editor.getText(), value)
					: null;
				valueRef.current = value;
				setActiveFile((current) => ({ ...current, content: value }));
				if (!readOnly && editor && replacement) editor.applyEdits([replacement]);
			},
			getPath: () => pathRef.current,
			getPosition: () => {
				if (!readOnly && editorRef.current) {
					positionRef.current = primaryHandlePosition(editorRef.current.getState());
				}
				return positionRef.current;
			},
			switchFile: (nextPath, nextContent, nextFocusLine, nextFocusColumn) => {
				setSelectedLines(null);
				pathRef.current = nextPath;
				valueRef.current = nextContent;
				positionRef.current =
					nextFocusLine != null
						? { lineNumber: nextFocusLine, column: nextFocusColumn ?? 1 }
						: null;
				pendingPositionRef.current = positionRef.current;
				setActiveFile({ path: nextPath, content: nextContent });
				return true;
			},
			layout: () => {
				virtualizerRef.current?.markDOMDirty();
				window.dispatchEvent(new Event("resize"));
			},
			focus: () => {
				const target = positionRef.current;
				editorRef.current?.focus(
					target
						? { lineNumber: target.lineNumber, character: target.column - 1 }
						: { lineNumber: "first-visible" },
				);
			},
		}),
		[readOnly],
	);

	useLayoutEffect(() => {
		if (pathRef.current === path && (reuseInstance || !readOnly)) return;
		pathRef.current = path;
		valueRef.current = content;
		setSelectedLines(null);
		const targetLine = cursorLine ?? focusLine;
		positionRef.current =
			targetLine != null
				? { lineNumber: targetLine, column: cursorColumn ?? 1 }
				: null;
		pendingPositionRef.current = positionRef.current;
		setActiveFile({ path, content });
	}, [content, cursorColumn, cursorLine, focusLine, path, readOnly, reuseInstance]);

	useLayoutEffect(() => {
		if (readOnly) return;
		const target = pendingPositionRef.current;
		if (!target) return;
		const frame = requestAnimationFrame(() => {
			const editor = editorRef.current;
			if (!editor || editor.getFile()?.name !== activeFile.path) return;
			editor.setState(collapsedEditorState(target.lineNumber, target.column));
			editor.focus({
				lineNumber: target.lineNumber,
				character: target.column - 1,
			});
		});
		return () => cancelAnimationFrame(frame);
	}, [activeFile.path, readOnly]);

	useEffect(() => {
		editorRef.current?.setOptions({
			enabledSelectionAction: Boolean(onAnnotate),
			renderSelectionAction,
		});
	}, [onAnnotate, renderSelectionAction]);

	useEffect(
		() => () => {
			editorRef.current?.cleanUp();
			editorRef.current = null;
			virtualizerRef.current = undefined;
		},
		[],
	);

	const file = useMemo(
		() => ({
			name: activeFile.path,
			contents: pathRef.current === activeFile.path ? valueRef.current : activeFile.content,
			cacheKey: activeFile.path,
		}),
		[activeFile, readOnly],
	);
	const fileOptions = useMemo<ComponentProps<typeof File>["options"]>(
		() => ({
			disableFileHeader: true,
			overflow: "scroll",
			theme: theme === "dark" ? "pierre-dark" : "pierre-light",
			themeType: theme,
			tokenizeMaxLineLength: 2_000,
			tokenizeMaxLength: 250_000,
			lineHoverHighlight: onAnnotate ? "both" : "line",
			enableGutterUtility: Boolean(onAnnotate),
			enableLineSelection: Boolean(onAnnotate && readOnly),
			onLineSelected: onAnnotate && readOnly ? setSelectedLines : undefined,
			onGutterUtilityClick: onAnnotate
				? (range) => annotateSelectedLines(range, null)
				: undefined,
		}),
		[annotateSelectedLines, onAnnotate, readOnly, theme],
	);
	return (
		<div ref={hostRef} className="relative flex min-h-0 flex-1 overflow-hidden bg-background">
			<EditProvider createEditor={createEditor}>
				<Virtualizer
					className="h-full min-h-0 min-w-0 flex-1 overflow-auto"
					contentClassName="min-h-full min-w-full"
					config={{ overscrollSize: 600 }}
				>
					<WorkspaceFileRenderer
						file={file}
						readOnly={readOnly}
						options={fileOptions}
						virtualizerRef={virtualizerRef}
						selectedLines={readOnly ? selectedLines : null}
					/>
				</Virtualizer>
			</EditProvider>
			{readOnly && selectedLines && onAnnotate ? (
				<button
					type="button"
					className="absolute bottom-3 right-3 z-20 inline-flex items-center gap-1.5 rounded-md bg-primary px-2.5 py-1.5 text-[11px] text-primary-foreground shadow-md"
					onClick={(event) => annotateSelectedLines(selectedLines, event.currentTarget)}
				>
					<MessageSquare className="size-3" aria-hidden />
					{annotateLabel}
				</button>
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
		initialBuffer,
		onBufferSnapshot,
		useSharedEditor = false,
		editorBridge,
		onContentReady,
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
	const focusColumn = source.kind === "path" ? source.focusColumn : null;
	const canEdit = Boolean(editable && workspaceRoot);

	const editorRef = useRef<FileEditorHandle | null>(null);
	const resolveEditor = useCallback(
		() => editorBridge?.current ?? editorRef.current,
		[editorBridge],
	);
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
		const value = resolveEditor()?.getValue() ?? "";
		setDirty(value !== baseContentRef.current);
	}, [resolveEditor]);

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
		const mine = resolveEditor()?.getValue() ?? "";
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
	}, [canEdit, filePath, handleEditorSaved, resolveEditor, saving, t, workspaceRoot]);

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
		resolveEditor()?.setValue(reconcile.disk);
		baseContentRef.current = reconcile.disk;
		setDirty(false);
		setReconcile(null);
	}, [reconcile, resolveEditor]);

	// Let the tab wrapper drive Save on the active surface and watch its state.
	useImperativeHandle(
		ref,
		() => ({
			save: () => void handleSave(),
			reveal: () => {
				resolveEditor()?.layout();
				resolveEditor()?.focus();
			},
			syncEditorChange: () => handleEditorChange(),
		}),
		[handleEditorChange, handleSave, resolveEditor],
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
	const editorContent =
		initialBuffer != null && initialBuffer.length > 0 ? initialBuffer : body;
	const contentState = resolveFileSurfaceContentState(query);
	const contentReadySentRef = useRef(false);

	useEffect(() => {
		contentReadySentRef.current = false;
	}, [filePath]);

	// Notify the shared editor once when disk content is first available. Do not
	// re-fire on buffer edits — that would push content back into the editor and reset
	// the cursor while the user is typing.
	useEffect(() => {
		if (contentState !== "editor" || !onContentReady || body.length === 0) {
			return;
		}
		if (contentReadySentRef.current) {
			return;
		}
		contentReadySentRef.current = true;
		const initialContent =
			initialBuffer != null && initialBuffer.length > 0 ? initialBuffer : body;
		onContentReady(initialContent);
	}, [body, contentState, initialBuffer, onContentReady]);

	// The loaded disk body becomes the reconciliation baseline. Fires only when the
	// content value actually changes (new file, or a refetch with new content).
	useEffect(() => {
		baseContentRef.current = body;
		if (initialBuffer == null) {
			setDirty(false);
			return;
		}
		setDirty(initialBuffer !== body);
	}, [body, initialBuffer]);

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
				) : !useSharedEditor && contentState === "editor" ? (
					<WorkspaceFileEditor
						ref={editorRef}
						path={filePath}
						content={editorContent}
						focusLine={focusLine ?? null}
						cursorColumn={focusColumn ?? null}
						readOnly={!canEdit}
						onChange={canEdit ? handleEditorChange : undefined}
						onAnnotate={annotationsEnabled ? handleAnnotate : undefined}
						annotateLabel={t("diffAnnotate.sendToAgent")}
					/>
				) : useSharedEditor && contentState === "editor" ? (
					<div className="min-h-0 flex-1 bg-background" aria-hidden />
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
					{open ? (
						<WorkspaceChangesDiffLoader
							path={path}
							originalText={disk}
							modifiedText={mine}
							inline={false}
						/>
					) : null}
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
