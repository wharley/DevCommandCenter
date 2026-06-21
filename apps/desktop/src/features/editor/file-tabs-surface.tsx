import { Loader2, Save, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { shouldIgnoreGlobalShortcutTarget } from "@/features/shortcuts/shortcut-utils";
import { cn } from "@/lib/utils";
import type { DiffAnnotationPayload } from "@/lib/monaco-runtime";
import type {
	DiffAnnotationRequest,
	DiffAnnotationSubmit,
} from "./diff-annotation";
import {
	DiffAnnotationPopover,
	type PendingAnnotation,
} from "./diff-annotation";
import { hasDirtyFileSurfaceState } from "./file-surface.logic";
import {
	WorkspaceFileEditor,
	WorkspaceFileSurface,
	type FileEditorHandle,
	type WorkspaceFileSurfaceHandle,
} from "./WorkspaceFileSurface";

type OpenFile = {
	path: string;
	name: string;
	focusLine: number | null;
	/** Preview tabs (italic) are replaced by the next preview; editing pins them. */
	preview: boolean;
};

type SurfaceState = { dirty: boolean; saving: boolean };

type FileTabsSurfaceProps = {
	workspaceRoot: string | null;
	/** The current open request from the app (one file-edit selection). */
	path: string;
	name: string;
	openRequestId: number;
	focusLine?: number | null;
	/** Close the whole surface (no tabs left, or the user closed it). */
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

export function FileTabsSurface({
	workspaceRoot,
	path,
	name,
	openRequestId,
	focusLine,
	onClose,
	onSubmitAnnotation,
	onEditInComposer,
	onAddToReview,
}: FileTabsSurfaceProps) {
	const { t } = useTranslation("common");
	const [pending, setPending] = useState<PendingAnnotation | null>(null);
	const annotationsEnabled = Boolean(
		onSubmitAnnotation || onEditInComposer || onAddToReview,
	);
	const [openFiles, setOpenFiles] = useState<OpenFile[]>(() => [
		{ path, name, focusLine: focusLine ?? null, preview: true },
	]);
	const [activePath, setActivePath] = useState<string>(path);
	const [stateByPath, setStateByPath] = useState<Record<string, SurfaceState>>(
		{},
	);

	const surfaceRefs = useRef(
		new Map<string, WorkspaceFileSurfaceHandle | null>(),
	);
	const buffersRef = useRef(new Map<string, string>());
	const contentByPathRef = useRef(new Map<string, string>());
	const cursorByPathRef = useRef(
		new Map<string, { lineNumber: number; column: number }>(),
	);
	const sharedEditorRef = useRef<FileEditorHandle | null>(null);
	const editorSwitchLockRef = useRef(false);
	const previousActivePathRef = useRef(activePath);
	const [sharedEditorStarted, setSharedEditorStarted] = useState(false);
	const [sharedEditorPayload, setSharedEditorPayload] = useState<{
		path: string;
		content: string;
		focusLine: number | null;
		cursorLine: number | null;
		cursorColumn: number | null;
	} | null>(null);
	// Mirrors for use inside global key/close handlers without stale closures.
	const activePathRef = useRef(activePath);
	activePathRef.current = activePath;
	const stateByPathRef = useRef(stateByPath);
	stateByPathRef.current = stateByPath;

	const saveEditorState = useCallback((filePath: string) => {
		const editor = sharedEditorRef.current;
		if (!editor || editor.getPath() !== filePath) {
			return;
		}
		buffersRef.current.set(filePath, editor.getValue());
		const position = editor.getPosition();
		if (position) {
			cursorByPathRef.current.set(filePath, position);
		}
	}, []);

	const activateTab = useCallback(
		(filePath: string) => {
			const leaving = activePathRef.current;
			if (leaving !== filePath) {
				saveEditorState(leaving);
			}
			editorSwitchLockRef.current = true;
			setActivePath(filePath);
		},
		[saveEditorState],
	);

	// React to a new open request: focus an existing tab, or add a fresh preview
	// (replacing the previous preview so previews never pile up).
	useEffect(() => {
		setOpenFiles((prev) => {
			const existing = prev.find((file) => file.path === path);
			if (existing) {
				if (focusLine != null) {
					cursorByPathRef.current.delete(path);
				}
				return prev.map((file) =>
					file.path === path ? { ...file, focusLine: focusLine ?? null } : file,
				);
			}
			const withoutPreview = prev.filter((file) => !file.preview);
			return [
				...withoutPreview,
				{ path, name, focusLine: focusLine ?? null, preview: true },
			];
		});
		activateTab(path);
	}, [path, name, openRequestId, focusLine, activateTab]);

	const handleBufferSnapshot = useCallback((filePath: string, content: string) => {
		buffersRef.current.set(filePath, content);
	}, []);

	const applySharedEditorPayload = useCallback(
		(filePath: string, diskContent: string) => {
			const file = openFiles.find((entry) => entry.path === filePath);
			const buffered = buffersRef.current.get(filePath);
			const content = buffered ?? diskContent;
			const savedCursor = cursorByPathRef.current.get(filePath);
			const focusLine = file?.focusLine ?? null;
			const cursorLine = savedCursor?.lineNumber ?? focusLine;
			const cursorColumn =
				savedCursor?.column ?? (focusLine != null ? 1 : null);

			setSharedEditorPayload((prev) =>
				prev?.path === filePath &&
				prev.content === content &&
				prev.focusLine === focusLine &&
				prev.cursorLine === cursorLine &&
				prev.cursorColumn === cursorColumn
					? prev
					: {
							path: filePath,
							content,
							focusLine,
							cursorLine,
							cursorColumn,
						},
			);
			setSharedEditorStarted(true);
		},
		[openFiles],
	);

	const handleContentReady = useCallback(
		(filePath: string, content: string) => {
			if (contentByPathRef.current.has(filePath)) {
				return;
			}
			contentByPathRef.current.set(filePath, content);
			if (filePath !== activePathRef.current) {
				return;
			}
			applySharedEditorPayload(filePath, content);
		},
		[applySharedEditorPayload],
	);

	useEffect(() => {
		const leaving = previousActivePathRef.current;
		if (leaving !== activePath) {
			saveEditorState(leaving);
		}
		previousActivePathRef.current = activePath;

		const cached = contentByPathRef.current.get(activePath);
		if (cached !== undefined) {
			applySharedEditorPayload(activePath, cached);
		}

		const unlockFrame = requestAnimationFrame(() => {
			editorSwitchLockRef.current = false;
		});
		return () => cancelAnimationFrame(unlockFrame);
	}, [activePath, applySharedEditorPayload, saveEditorState]);

	const handleStateChange = useCallback((filePath: string, next: SurfaceState) => {
		setStateByPath((prev) => {
			const current = prev[filePath];
			if (current && current.dirty === next.dirty && current.saving === next.saving) {
				return prev;
			}
			return { ...prev, [filePath]: next };
		});
		// Editing a preview tab pins it so the next preview doesn't replace it.
		if (next.dirty) {
			setOpenFiles((prev) =>
				prev.map((file) =>
					file.path === filePath && file.preview
						? { ...file, preview: false }
						: file,
				),
			);
		}
	}, []);

	const closeTab = useCallback(
		(filePath: string) => {
			if (
				stateByPathRef.current[filePath]?.dirty &&
				!window.confirm(t("fileSurface.discardConfirm"))
			) {
				return;
			}
			surfaceRefs.current.delete(filePath);
			buffersRef.current.delete(filePath);
			contentByPathRef.current.delete(filePath);
			cursorByPathRef.current.delete(filePath);
			setStateByPath((prev) => {
				const { [filePath]: _removed, ...rest } = prev;
				return rest;
			});
			setOpenFiles((prev) => {
				const index = prev.findIndex((file) => file.path === filePath);
				const next = prev.filter((file) => file.path !== filePath);
				if (next.length === 0) {
					onClose();
					return next;
				}
				if (activePathRef.current === filePath) {
					const neighbor = next[Math.min(index, next.length - 1)];
					activateTab(neighbor.path);
				}
				return next;
			});
		},
		[activateTab, onClose, t],
	);

	const pinTab = useCallback((filePath: string) => {
		setOpenFiles((prev) =>
			prev.map((file) =>
				file.path === filePath ? { ...file, preview: false } : file,
			),
		);
	}, []);

	const requestCloseAll = useCallback(() => {
		const anyDirty = hasDirtyFileSurfaceState(stateByPathRef.current);
		if (anyDirty && !window.confirm(t("fileSurface.discardConfirm"))) {
			return;
		}
		onClose();
	}, [onClose, t]);

	const handleAnnotate = useCallback((payload: DiffAnnotationPayload) => {
		const { anchor, ...rest } = payload;
		setPending({
			request: { ...rest, path: activePathRef.current },
			anchor,
		});
	}, []);

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
			setPending(null);
		},
		[onAddToReview, pending],
	);

	// One global key handler for the active tab (embedded surfaces stay silent).
	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.defaultPrevented) return;
			if (
				(event.metaKey || event.ctrlKey) &&
				!event.shiftKey &&
				!event.altKey &&
				event.key.toLowerCase() === "s"
			) {
				event.preventDefault();
				surfaceRefs.current.get(activePathRef.current)?.save();
				return;
			}
			if (event.key !== "Escape") return;
			if (shouldIgnoreGlobalShortcutTarget(event.target)) return;
			event.preventDefault();
			if (pending) {
				setPending(null);
				return;
			}
			requestCloseAll();
		};
		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [pending, requestCloseAll]);

	// On tab switch, re-measure + focus the now-visible editor (kept-alive surfaces
	// can render at zero size while hidden under display:none).
	useEffect(() => {
		const revealActive = () => {
			surfaceRefs.current.get(activePath)?.reveal();
		};
		const frame = requestAnimationFrame(() => {
			revealActive();
		});
		const quickTimeout = window.setTimeout(revealActive, 0);
		const settledTimeout = window.setTimeout(revealActive, 80);
		return () => {
			cancelAnimationFrame(frame);
			window.clearTimeout(quickTimeout);
			window.clearTimeout(settledTimeout);
		};
	}, [activePath, openRequestId]);

	const activeState = stateByPath[activePath];

	return (
		<section
			aria-label={t("fileSurface.ariaLabel")}
			data-focus-scope="editor"
			className="flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground"
		>
			<div className="flex h-9 items-center border-b border-border">
				<div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto px-2">
					{openFiles.map((file) => (
						<Tab
							key={file.path}
							file={file}
							active={file.path === activePath}
							dirty={Boolean(stateByPath[file.path]?.dirty)}
							closeLabel={t("fileTabs.close")}
							onActivate={() => activateTab(file.path)}
							onPin={() => pinTab(file.path)}
							onClose={() => closeTab(file.path)}
						/>
					))}
				</div>
				<div className="flex shrink-0 items-center gap-1 pr-2">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={() => surfaceRefs.current.get(activePath)?.save()}
						disabled={!activeState?.dirty || activeState?.saving}
						aria-label={t("fileSurface.save")}
						className="gap-1.5 px-2 text-muted-foreground hover:text-foreground"
					>
						{activeState?.saving ? (
							<Loader2 className="size-3.5 animate-spin" strokeWidth={1.8} />
						) : (
							<Save className="size-3.5" strokeWidth={1.8} />
						)}
						<span>{t("fileSurface.save")}</span>
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={requestCloseAll}
						aria-label={t("fileTabs.closeAll")}
						className="px-2 text-muted-foreground hover:text-foreground"
					>
						<X className="size-3.5" strokeWidth={1.8} />
					</Button>
				</div>
			</div>
			<div className="relative min-h-0 flex-1 bg-background">
				{openFiles.map((file) => (
					<div
						key={file.path}
						aria-hidden={file.path !== activePath}
						className={cn(
							"absolute inset-0 flex min-h-0 flex-col",
							file.path === activePath
								? "z-10 opacity-100"
								: "pointer-events-none invisible z-0 opacity-0",
						)}
					>
						<WorkspaceFileSurface
							ref={(handle) => {
								if (handle) {
									surfaceRefs.current.set(file.path, handle);
								} else {
									surfaceRefs.current.delete(file.path);
								}
							}}
							embedded
							editable
							useSharedEditor
							editorBridge={
								file.path === activePath ? sharedEditorRef : undefined
							}
							workspaceRoot={workspaceRoot}
							initialBuffer={
								buffersRef.current.has(file.path)
									? (buffersRef.current.get(file.path) ?? null)
									: null
							}
							onBufferSnapshot={(content) =>
								handleBufferSnapshot(file.path, content)
							}
							onContentReady={(content) => handleContentReady(file.path, content)}
							source={{
								kind: "path",
								path: file.path,
								name: file.name,
								focusLine: file.focusLine,
							}}
							onStateChange={(next) => handleStateChange(file.path, next)}
							onClose={onClose}
							onSubmitAnnotation={onSubmitAnnotation}
							onEditInComposer={onEditInComposer}
							onAddToReview={onAddToReview}
						/>
					</div>
				))}
				{sharedEditorStarted ? (
					<div
						className={cn(
							"absolute inset-0 z-20 flex min-h-0 flex-col",
							(sharedEditorPayload?.path ?? activePath) !== activePath &&
								"pointer-events-none invisible",
						)}
					>
						<WorkspaceFileEditor
							ref={sharedEditorRef}
							path={sharedEditorPayload?.path ?? activePath}
							content={
								sharedEditorPayload?.content ??
								contentByPathRef.current.get(activePath) ??
								""
							}
							focusLine={sharedEditorPayload?.focusLine ?? null}
							cursorLine={sharedEditorPayload?.cursorLine ?? null}
							cursorColumn={sharedEditorPayload?.cursorColumn ?? null}
							readOnly={false}
							reuseInstance
							onChange={() => {
								if (editorSwitchLockRef.current) {
									return;
								}
								const currentPath = activePathRef.current;
								const editorPath = sharedEditorRef.current?.getPath();
								if (!editorPath || editorPath !== currentPath) {
									return;
								}
								const value = sharedEditorRef.current?.getValue() ?? "";
								buffersRef.current.set(currentPath, value);
								surfaceRefs.current.get(currentPath)?.syncEditorChange();
							}}
							onAnnotate={annotationsEnabled ? handleAnnotate : undefined}
							annotateLabel={t("diffAnnotate.sendToAgent")}
						/>
					</div>
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

function Tab({
	file,
	active,
	dirty,
	closeLabel,
	onActivate,
	onPin,
	onClose,
}: {
	file: OpenFile;
	active: boolean;
	dirty: boolean;
	closeLabel: string;
	onActivate: () => void;
	onPin: () => void;
	onClose: () => void;
}) {
	return (
		<div
			role="tab"
			aria-selected={active}
			onClick={onActivate}
			onDoubleClick={onPin}
			title={file.path}
			className={cn(
				"group flex h-7 max-w-[170px] shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2 text-[11px] transition-colors",
				active
					? "bg-muted/60 text-foreground shadow-sm"
					: "text-muted-foreground hover:bg-muted/30 hover:text-foreground",
			)}
		>
			{dirty ? (
				<span className="size-1.5 shrink-0 rounded-full bg-primary" aria-hidden />
			) : null}
			<span className={cn("truncate", file.preview && "italic")}>
				{file.name}
			</span>
			<button
				type="button"
				aria-label={closeLabel}
				onClick={(event) => {
					event.stopPropagation();
					onClose();
				}}
				className={cn(
					"shrink-0 rounded p-0.5 text-muted-foreground hover:text-foreground",
					dirty ? "opacity-100" : "opacity-0 group-hover:opacity-100",
				)}
			>
				<X className="size-3" strokeWidth={2} />
			</button>
		</div>
	);
}
