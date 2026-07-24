import "monaco-editor/min/vs/editor/editor.main.css";
import type * as Monaco from "monaco-editor";
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import cssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import htmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";

type MonacoModule = typeof Monaco;
type StandaloneEditor = Monaco.editor.IStandaloneCodeEditor;
type StandaloneDiffEditor = Monaco.editor.IStandaloneDiffEditor;

type MonacoRuntime = {
	monaco: MonacoModule;
};

type DisposableLike = {
	dispose(): void;
};

type FileEditorController = {
	editor: StandaloneEditor;
	dispose(): void;
	getValue(): string;
	setValue(value: string): void;
	revealPosition(line?: number, column?: number): void;
	onDidChangeModelContent(callback: (value: string) => void): DisposableLike;
	getPath(): string;
	switchFile(
		path: string,
		content?: string,
		line?: number,
		column?: number,
	): boolean;
};

type DiffEditorController = {
	editor: StandaloneDiffEditor;
	dispose(): void;
	setTexts(options: {
		originalText: string;
		modifiedText: string;
		inline: boolean;
	}): void;
	setMachineAnnotations(annotations: DiffMachineAnnotation[]): void;
	revealLine(line: number, side?: DiffMachineAnnotation["side"]): void;
};

type DiffMachineAnnotationClick = {
	annotation: DiffMachineAnnotation;
	anchor: { top: number; left: number };
};

/**
 * Emitted when the reviewer selects a region in the diff and triggers the
 * "send to agent" affordance. Line numbers are 1-based and refer to the
 * modified document; `snippet` is the exact selected text.
 */
export type DiffAnnotationPayload = {
	/** "original" = deleted/old side, "modified" = added/new side. */
	side: "original" | "modified";
	startLine: number;
	endLine: number;
	snippet: string;
	/** Viewport coordinates of the trigger button, for anchoring an overlay. */
	anchor: { top: number; left: number };
};

export type DiffMachineAnnotation = {
	source: "coderabbit" | "forge-review";
	id?: string;
	severity: "critical" | "major" | "minor" | "trivial" | "info" | "unknown";
	side: "original" | "modified";
	startLine: number;
	endLine: number;
	title: string;
};

let runtimePromise: Promise<MonacoRuntime> | null = null;
const fileContentCache = new Map<string, string>();
let editorModelId = 0;

type EditorTheme = "light" | "dark";
let desiredTheme: EditorTheme = detectInitialTheme();

function detectInitialTheme(): EditorTheme {
	if (typeof document === "undefined") {
		return "dark";
	}

	return document.documentElement.classList.contains("dark") ? "dark" : "light";
}

function themeId(theme: EditorTheme): string {
	return theme === "dark" ? "dcc-editor-dark" : "dcc-editor-light";
}

function createEditorModelUri(
	monaco: MonacoModule,
	path: string,
	role: string,
): Monaco.Uri {
	editorModelId += 1;
	return monaco.Uri.file(path).with({
		query: `dcc-editor-role=${role}&dcc-editor-id=${editorModelId}`,
	});
}

export async function createFileEditor(options: {
	container: HTMLElement;
	path: string;
	content: string;
	line?: number;
	column?: number;
	/** Render the file without allowing edits (e.g. read-only review surface). */
	readOnly?: boolean;
	/**
	 * When provided, a floating "send to agent" button is shown over the editor
	 * whenever the user selects a non-empty range. Mirrors the diff affordance.
	 */
	onAnnotate?: (payload: DiffAnnotationPayload) => void;
	/** Label for the annotate button; defaults to a pt-BR string. */
	annotateLabel?: string;
}): Promise<FileEditorController> {
	const runtime = await ensureRuntime();
	const { monaco } = runtime;
	const language = resolveLanguageId(monaco, options.path);
	const modelUri = createEditorModelUri(monaco, options.path, "file");
	const model = monaco.editor.createModel(options.content, language, modelUri);

	fileContentCache.set(options.path, options.content);

	const readOnly = options.readOnly ?? false;
	const editor = monaco.editor.create(options.container, {
		automaticLayout: true,
		accessibilitySupport: "off",
		bracketPairColorization: { enabled: true },
		contextmenu: true,
		cursorBlinking: "blink",
		cursorSmoothCaretAnimation: "on",
		detectIndentation: true,
		domReadOnly: readOnly,
	// EditContext (Monaco 0.53+) breaks keyboard input in embedded shells like Tauri;
	// keep the classic textarea input path. See docs/MONACO_TAURI.md.
	editContext: false,
		dragAndDrop: true,
		folding: true,
		formatOnPaste: true,
		formatOnType: true,
		fontFamily:
			'"SF Mono","Monaco","Cascadia Mono","Roboto Mono","Menlo",monospace',
		fontLigatures: true,
		fontSize: 13,
		glyphMargin: false,
		lineHeight: 21,
		links: true,
		minimap: { enabled: false },
		model,
		mouseWheelZoom: false,
		multiCursorModifier: "alt",
		padding: { top: 14, bottom: 24 },
		readOnly,
		renderValidationDecorations: "off",
		renderWhitespace: "selection",
		roundedSelection: false,
		scrollBeyondLastLine: false,
		scrollbar: {
			alwaysConsumeMouseWheel: false,
			horizontal: "auto",
			vertical: "auto",
		},
		selectOnLineNumbers: true,
		smoothScrolling: true,
		tabSize: 2,
		tabFocusMode: false,
		autoIndent: "full",
		insertSpaces: true,
		theme: themeId(desiredTheme),
		useTabStops: true,
		wordWrap: "on",
	});

	revealEditorPosition(editor, options.line, options.column);

	options.container.dataset.dccMonacoInput = editor.getOption(
		monaco.editor.EditorOption.editContext,
	)
		? "edit-context"
		: "textarea";

	const editorDisposables: DisposableLike[] = [
		...installCodeEditorFocusGuards(editor),
		...installEmbeddedShellNavigationCommands(monaco, editor),
	];
	const annotateDisposables: DisposableLike[] = [];
	if (options.onAnnotate) {
		annotateDisposables.push(
			attachAnnotateButton(monaco, editor, "modified", {
				label: options.annotateLabel ?? "Enviar ao agente ↗",
				onAnnotate: options.onAnnotate,
			}),
		);
	}

	const currentModel = model;
	let activePath = options.path;

	return {
		editor,
		dispose() {
			releaseCodeEditorFocus(editor);
			for (const disposable of editorDisposables) {
				disposable.dispose();
			}
			for (const disposable of annotateDisposables) {
				disposable.dispose();
			}
			editor.dispose();
			currentModel.dispose();
		},
		getValue() {
			return currentModel.getValue();
		},
		setValue(value: string) {
			if (currentModel.getValue() === value) {
				return;
			}

			currentModel.setValue(value);
		},
		revealPosition(line?: number, column?: number) {
			revealEditorPosition(editor, line, column);
		},
		onDidChangeModelContent(callback) {
			return currentModel.onDidChangeContent(() => {
				callback(currentModel.getValue());
			});
		},
		getPath() {
			return activePath;
		},
		switchFile(path: string, content?: string, line?: number, column?: number) {
			const resolvedContent = content ?? fileContentCache.get(path);
			if (resolvedContent === undefined) {
				return false;
			}

			const samePath = path === activePath;
			const position = editor.getPosition();
			const selection = editor.getSelection();
			const didSetValue = currentModel.getValue() !== resolvedContent;
			activePath = path;

			if (didSetValue) {
				currentModel.setValue(resolvedContent);
			}

			const nextLanguage = resolveLanguageId(monaco, path);
			if (nextLanguage && currentModel.getLanguageId() !== nextLanguage) {
				monaco.editor.setModelLanguage(currentModel, nextLanguage);
			}

			fileContentCache.set(path, resolvedContent);

			if (line) {
				revealEditorPosition(editor, line, column);
			} else if (samePath && position) {
				editor.setPosition(position);
				if (selection) {
					editor.setSelection(selection);
				}
			}

			focusCodeEditor(editor);
			return true;
		},
	};
}

export async function createDiffEditor(options: {
	container: HTMLElement;
	path: string;
	originalText: string;
	modifiedText: string;
	inline: boolean;
	focusLine?: number | null;
	machineAnnotations?: DiffMachineAnnotation[];
	/**
	 * When provided, a floating "send to agent" button is shown over either
	 * side of the diff whenever the reviewer selects a non-empty range.
	 */
	onAnnotate?: (payload: DiffAnnotationPayload) => void;
	/** Label for the annotate button; defaults to a pt-BR string. */
	annotateLabel?: string;
	/** Label for PR review comment affordances. */
	reviewCommentLabel?: string;
	onMachineAnnotationClick?: (payload: DiffMachineAnnotationClick) => void;
}): Promise<DiffEditorController> {
	const runtime = await ensureRuntime();
	const { monaco } = runtime;
	const language = resolveLanguageId(monaco, options.path);

	const originalUri = createEditorModelUri(monaco, options.path, "diff-original");
	const modifiedUri = createEditorModelUri(monaco, options.path, "diff-modified");

	const originalModel = monaco.editor.createModel(
		options.originalText,
		language,
		originalUri,
	);
	const modifiedModel = monaco.editor.createModel(
		options.modifiedText,
		language,
		modifiedUri,
	);

	const editor = monaco.editor.createDiffEditor(options.container, {
		automaticLayout: true,
		editContext: false,
		enableSplitViewResizing: true,
		fontFamily:
			'"SF Mono","Monaco","Cascadia Mono","Roboto Mono","Menlo",monospace',
		fontLigatures: true,
		fontSize: 13,
		hideUnchangedRegions: {
			enabled: true,
			contextLineCount: 4,
			minimumLineCount: 2,
			revealLineCount: 3,
		},
		lineHeight: 21,
		minimap: { enabled: false },
		originalEditable: false,
		padding: { top: 14, bottom: 24 },
		readOnly: true,
		renderOverviewRuler: false,
		renderSideBySide: !options.inline,
		scrollBeyondLastLine: false,
		smoothScrolling: true,
		glyphMargin: true,
		theme: themeId(desiredTheme),
	});

	editor.setModel({
		original: originalModel,
		modified: modifiedModel,
	});

	ensureDiffMachineAnnotationStyles();
	const machineAnnotationController = createDiffMachineAnnotationController(
		monaco,
		editor,
		options.onMachineAnnotationClick,
		options.reviewCommentLabel ?? "Comentário",
	);
	machineAnnotationController.set(options.machineAnnotations ?? []);
	if (options.focusLine) {
		revealDiffLine(editor, options.focusLine, "modified");
	}

	const originalEditor = editor.getOriginalEditor();
	const modifiedEditor = editor.getModifiedEditor();
	const editorDisposables: DisposableLike[] = [
		...installCodeEditorFocusGuards(originalEditor),
		...installCodeEditorFocusGuards(modifiedEditor),
		...installEmbeddedShellNavigationCommands(monaco, originalEditor),
		...installEmbeddedShellNavigationCommands(monaco, modifiedEditor),
	];

	const annotateDisposables: DisposableLike[] = [];
	if (options.onAnnotate) {
		annotateDisposables.push(
			attachDiffAnnotateAffordance(monaco, editor, {
				label: options.annotateLabel ?? "Enviar ao agente ↗",
				onAnnotate: options.onAnnotate,
			}),
		);
	}

	return {
		editor,
		dispose() {
			for (const disposable of editorDisposables) {
				disposable.dispose();
			}
			for (const disposable of annotateDisposables) {
				disposable.dispose();
			}
			machineAnnotationController.dispose();
			editor.dispose();
			originalModel.dispose();
			modifiedModel.dispose();
		},
		setTexts({ originalText, modifiedText, inline }) {
			if (originalModel.getValue() !== originalText) {
				originalModel.setValue(originalText);
			}
			if (modifiedModel.getValue() !== modifiedText) {
				modifiedModel.setValue(modifiedText);
			}
			editor.updateOptions({ renderSideBySide: !inline });
		},
		setMachineAnnotations(annotations) {
			machineAnnotationController.set(annotations);
		},
		revealLine(line, side = "modified") {
			revealDiffLine(editor, line, side);
		},
	};
}

function createDiffMachineAnnotationController(
	monaco: MonacoModule,
	diffEditor: StandaloneDiffEditor,
	onClick?: (payload: DiffMachineAnnotationClick) => void,
	reviewCommentLabel = "Comentário",
): { set(annotations: DiffMachineAnnotation[]): void; dispose(): void } {
	const originalDecorations = diffEditor
		.getOriginalEditor()
		.createDecorationsCollection();
	const modifiedDecorations = diffEditor
		.getModifiedEditor()
		.createDecorationsCollection();
	let activeAnnotations: DiffMachineAnnotation[] = [];
	let reviewCommentWidgets: Array<{
		editor: StandaloneEditor;
		widget: Monaco.editor.IContentWidget;
	}> = [];

	const clearReviewCommentWidgets = () => {
		for (const { editor, widget } of reviewCommentWidgets) {
			editor.removeContentWidget(widget);
		}
		reviewCommentWidgets = [];
	};

	const toDecoration = (
		annotation: DiffMachineAnnotation,
	): Monaco.editor.IModelDeltaDecoration => {
		const startLine = Math.max(1, Math.floor(annotation.startLine));
		const endLine = Math.max(startLine, Math.floor(annotation.endLine));
		const isReviewComment = annotation.source === "forge-review";
		const label = isReviewComment ? "Code review" : "CodeRabbit";
		return {
			range: new monaco.Range(startLine, 1, endLine, 1),
			options: {
				isWholeLine: true,
				className: isReviewComment
					? "dcc-review-comment-line"
					: `dcc-coderabbit-line dcc-coderabbit-line-${annotation.severity}`,
				hoverMessage: {
					value: `**${label}**\n\n${annotation.title}`,
				},
			},
		};
	};

	const findAnnotationAt = (
		side: DiffMachineAnnotation["side"],
		lineNumber: number,
	) =>
		activeAnnotations.find(
			(annotation) =>
				annotation.source === "forge-review" &&
				annotation.side === side &&
				lineNumber >= Math.max(1, Math.floor(annotation.startLine)) &&
				lineNumber <= Math.max(1, Math.floor(annotation.endLine)),
		);

	const attachClick = (
		editor: StandaloneEditor,
		side: DiffMachineAnnotation["side"],
	): DisposableLike =>
		editor.onMouseDown((event) => {
			if (!onClick || !event.target.position) {
				return;
			}
			if (event.target.type !== monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN) {
				return;
			}
			const annotation = findAnnotationAt(side, event.target.position.lineNumber);
			if (!annotation) {
				return;
			}
			event.event.preventDefault();
			event.event.stopPropagation();
			onClick({
				annotation,
				anchor: {
					top: event.event.browserEvent.clientY,
					left: event.event.browserEvent.clientX,
				},
			});
		});

	const addReviewCommentWidget = (annotation: DiffMachineAnnotation) => {
		if (!onClick || annotation.source !== "forge-review") {
			return;
		}
		const editor =
			annotation.side === "original"
				? diffEditor.getOriginalEditor()
				: diffEditor.getModifiedEditor();
		const model = editor.getModel();
		if (!model) {
			return;
		}
		const lineNumber = Math.min(
			model.getLineCount(),
			Math.max(1, Math.floor(annotation.startLine)),
		);
		const column = 1;
		const button = document.createElement("button");
		button.type = "button";
		button.className = "dcc-review-comment-floating-button";
		button.title = annotation.title;
		button.setAttribute("aria-label", annotation.title);
		button.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M21 15a4 4 0 0 1-4 4H8l-5 3V7a4 4 0 0 1 4-4h10a4 4 0 0 1 4 4z"/></svg><span></span>`;
		const label = button.querySelector("span");
		if (label) {
			label.textContent = reviewCommentLabel;
		}
		button.addEventListener("mousedown", (event) => {
			event.preventDefault();
			event.stopPropagation();
		});
		button.addEventListener("click", (event) => {
			event.preventDefault();
			event.stopPropagation();
			const rect = button.getBoundingClientRect();
			onClick({
				annotation,
				anchor: { top: rect.top, left: rect.left },
			});
		});

		const widget: Monaco.editor.IContentWidget = {
			getId: () =>
				`dcc.diff.review-comment.widget.${annotation.side}.${annotation.id ?? lineNumber}`,
			getDomNode: () => button,
			getPosition: () => ({
				position: { lineNumber, column },
				preference: [
					monaco.editor.ContentWidgetPositionPreference.ABOVE,
					monaco.editor.ContentWidgetPositionPreference.BELOW,
				],
			}),
		};
		editor.addContentWidget(widget);
		editor.layoutContentWidget(widget);
		reviewCommentWidgets.push({ editor, widget });
	};

	const clickDisposables = [
		attachClick(diffEditor.getOriginalEditor(), "original"),
		attachClick(diffEditor.getModifiedEditor(), "modified"),
	];

	return {
		set(annotations) {
			clearReviewCommentWidgets();
			activeAnnotations = annotations;
			originalDecorations.set(
				annotations
					.filter((annotation) => annotation.side === "original")
					.map(toDecoration),
			);
			modifiedDecorations.set(
				annotations
					.filter((annotation) => annotation.side === "modified")
					.map(toDecoration),
			);
			for (const annotation of annotations) {
				addReviewCommentWidget(annotation);
			}
		},
		dispose() {
			clearReviewCommentWidgets();
			originalDecorations.clear();
			modifiedDecorations.clear();
			for (const disposable of clickDisposables) {
				disposable.dispose();
			}
		},
	};
}

function revealDiffLine(
	diffEditor: StandaloneDiffEditor,
	line: number,
	side: DiffMachineAnnotation["side"],
) {
	const editor =
		side === "original"
			? diffEditor.getOriginalEditor()
			: diffEditor.getModifiedEditor();
	const lineNumber = Math.max(1, Math.floor(line));
	editor.revealLineInCenterIfOutsideViewport(lineNumber);
	editor.setPosition({ lineNumber, column: 1 });
}

function ensureDiffMachineAnnotationStyles() {
	if (typeof document === "undefined") {
		return;
	}
	if (document.getElementById("dcc-coderabbit-diff-annotations")) {
		return;
	}
	const style = document.createElement("style");
	style.id = "dcc-coderabbit-diff-annotations";
	style.textContent = `
.monaco-editor .dcc-coderabbit-line {
	box-shadow: inset 2px 0 0 var(--border);
}
.monaco-editor .dcc-coderabbit-line-critical {
	background: color-mix(in srgb, var(--destructive) 18%, transparent);
	box-shadow: inset 2px 0 0 var(--destructive);
}
.monaco-editor .dcc-coderabbit-line-major {
	background: color-mix(in srgb, #f59e0b 16%, transparent);
	box-shadow: inset 2px 0 0 #f59e0b;
}
.monaco-editor .dcc-coderabbit-line-minor {
	background: color-mix(in srgb, #0284c7 14%, transparent);
	box-shadow: inset 2px 0 0 #0284c7;
}
.monaco-editor .dcc-coderabbit-line-trivial,
.monaco-editor .dcc-coderabbit-line-info,
.monaco-editor .dcc-coderabbit-line-unknown {
	background: color-mix(in srgb, var(--muted-foreground) 10%, transparent);
	box-shadow: inset 2px 0 0 var(--muted-foreground);
}
.monaco-editor .dcc-review-comment-line {
	background: color-mix(in oklch, #f59e0b 9%, transparent);
	box-shadow: inset 2px 0 0 color-mix(in oklch, #f59e0b 65%, transparent);
}
.dcc-review-comment-floating-button {
	display: inline-flex !important;
	flex-direction: row !important;
	align-items: center;
	justify-content: center;
	box-sizing: border-box;
	gap: 5px;
	height: 21px;
	padding: 0 9px;
	margin-bottom: 2px;
	font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif);
	font-size: 10.5px !important;
	font-weight: 600;
	letter-spacing: -0.01em;
	line-height: 1 !important;
	color: #92400e;
	background: color-mix(in oklch, #f59e0b 16%, var(--popover));
	border: 1px solid color-mix(in oklch, #f59e0b 50%, var(--border));
	border-radius: 999px;
	box-shadow:
		0 1px 2px rgba(0, 0, 0, 0.12),
		0 2px 8px rgba(0, 0, 0, 0.1);
	cursor: pointer;
	white-space: nowrap;
	transition:
		background 120ms ease,
		border-color 120ms ease,
		box-shadow 120ms ease,
		transform 120ms ease;
}
.dcc-review-comment-floating-button > span {
	display: inline-block;
	font-size: inherit;
	line-height: inherit;
	white-space: nowrap;
}
.dcc-review-comment-floating-button > svg {
	display: block;
	flex: 0 0 auto;
	width: 12px;
	height: 12px;
	fill: none;
	stroke: #b45309;
	stroke-width: 2;
	stroke-linecap: round;
	stroke-linejoin: round;
}
.dark .dcc-review-comment-floating-button {
	color: #fde68a;
	background: color-mix(in oklch, #f59e0b 20%, var(--popover));
	border-color: color-mix(in oklch, #f59e0b 42%, var(--border));
}
.dark .dcc-review-comment-floating-button > svg {
	stroke: #fcd34d;
}
.dcc-review-comment-floating-button:hover {
	background: color-mix(in oklch, #f59e0b 26%, var(--popover));
	border-color: color-mix(in oklch, #f59e0b 62%, var(--border));
	transform: translateY(-1px);
	box-shadow:
		0 2px 4px rgba(0, 0, 0, 0.16),
		0 4px 12px rgba(0, 0, 0, 0.14);
}
.dark .dcc-review-comment-floating-button:hover {
	background: color-mix(in oklch, #f59e0b 30%, var(--popover));
	border-color: color-mix(in oklch, #f59e0b 55%, var(--border));
}
.dcc-review-comment-floating-button:active {
	transform: translateY(0);
}
.dcc-review-comment-floating-button:focus-visible {
	outline: 2px solid var(--ring);
	outline-offset: 2px;
}
@media (prefers-reduced-motion: reduce) {
	.dcc-review-comment-floating-button {
		transition: none;
	}
	.dcc-review-comment-floating-button:hover {
		transform: none;
	}
}
`;
	document.head.appendChild(style);
}

/**
 * Wires the "send selection to agent" affordance onto both sides of a diff
 * editor. Confines all Monaco widget APIs here so the React layer only sees a
 * callback. Returns a disposable that tears down listeners and floating buttons.
 */
function attachDiffAnnotateAffordance(
	monaco: MonacoModule,
	diffEditor: StandaloneDiffEditor,
	options: {
		label: string;
		onAnnotate: (payload: DiffAnnotationPayload) => void;
	},
): DisposableLike {
	const disposables = [
		attachAnnotateButton(monaco, diffEditor.getModifiedEditor(), "modified", options),
		attachAnnotateButton(monaco, diffEditor.getOriginalEditor(), "original", options),
	];
	return {
		dispose() {
			for (const disposable of disposables) {
				disposable.dispose();
			}
		},
	};
}

function attachAnnotateButton(
	monaco: MonacoModule,
	editor: StandaloneEditor,
	side: DiffAnnotationPayload["side"],
	options: {
		label: string;
		onAnnotate: (payload: DiffAnnotationPayload) => void;
	},
): DisposableLike {
	let selection: Monaco.Selection | null = null;
	let mounted = false;

	const button = document.createElement("button");
	button.type = "button";
	button.textContent = options.label;
	button.setAttribute("aria-label", options.label);
	Object.assign(button.style, {
		display: "inline-flex",
		alignItems: "center",
		gap: "4px",
		padding: "3px 8px",
		fontSize: "11px",
		fontWeight: "600",
		lineHeight: "1.2",
		color: "var(--primary-foreground)",
		background: "var(--primary)",
		border: "1px solid var(--border)",
		borderRadius: "6px",
		boxShadow: "0 2px 8px rgba(0,0,0,0.25)",
		cursor: "pointer",
		whiteSpace: "nowrap",
	} satisfies Partial<CSSStyleDeclaration>);

	const widget: Monaco.editor.IContentWidget = {
		getId: () => `dcc.diff.annotate.widget.${side}`,
		getDomNode: () => button,
		getPosition: () =>
			selection
				? {
						position: {
							lineNumber: selection.startLineNumber,
							column: selection.startColumn,
						},
						preference: [
							monaco.editor.ContentWidgetPositionPreference.ABOVE,
							monaco.editor.ContentWidgetPositionPreference.BELOW,
						],
					}
				: null,
	};

	const hide = () => {
		selection = null;
		if (mounted) {
			editor.removeContentWidget(widget);
			mounted = false;
		}
	};

	// Prevent the mousedown from collapsing the editor selection before click.
	button.addEventListener("mousedown", (event) => {
		event.preventDefault();
		event.stopPropagation();
	});
	button.addEventListener("click", (event) => {
		event.preventDefault();
		const active = selection;
		const model = editor.getModel();
		if (!active || !model) {
			return;
		}
		const rect = button.getBoundingClientRect();
		options.onAnnotate({
			side,
			startLine: active.startLineNumber,
			endLine: active.endLineNumber,
			snippet: model.getValueInRange(active),
			anchor: { top: rect.top, left: rect.left },
		});
		hide();
	});

	const selectionListener = editor.onDidChangeCursorSelection((event) => {
		if (event.selection.isEmpty()) {
			hide();
			return;
		}
		selection = event.selection;
		if (!mounted) {
			editor.addContentWidget(widget);
			mounted = true;
		}
		editor.layoutContentWidget(widget);
	});

	return {
		dispose() {
			selectionListener.dispose();
			hide();
		},
	};
}

function releaseCodeEditorFocus(editor: StandaloneEditor) {
	const domNode = editor.getDomNode();
	const textarea = domNode?.querySelector("textarea.inputarea");
	if (textarea instanceof HTMLElement && document.activeElement === textarea) {
		textarea.blur();
	}
	if (domNode?.contains(document.activeElement)) {
		document.body.focus({ preventScroll: true });
	}
}

function focusCodeEditor(editor: StandaloneEditor) {
	editor.focus();
	const textarea = editor.getDomNode()?.querySelector("textarea.inputarea");
	if (textarea instanceof HTMLTextAreaElement) {
		textarea.focus({ preventScroll: true });
	}
	editor.layout();
}

function installEmbeddedShellNavigationCommands(
	monaco: MonacoModule,
	editor: StandaloneEditor,
): DisposableLike[] {
	// Default Monaco keybindings rely on context keys that can stay false in the
	// Tauri webview even when the textarea has focus. addCommand + trigger() uses
	// Monaco's built-in commands without DOM capture. See docs/MONACO_TAURI.md.
	const bind = (keybinding: number, command: string) => {
		editor.addCommand(keybinding, () => {
			editor.trigger("keyboard", command, null);
			focusCodeEditor(editor);
		});
	};

	bind(monaco.KeyCode.UpArrow, "cursorUp");
	bind(monaco.KeyCode.DownArrow, "cursorDown");
	bind(monaco.KeyCode.LeftArrow, "cursorLeft");
	bind(monaco.KeyCode.RightArrow, "cursorRight");
	bind(monaco.KeyCode.UpArrow | monaco.KeyMod.Shift, "cursorUpSelect");
	bind(monaco.KeyCode.DownArrow | monaco.KeyMod.Shift, "cursorDownSelect");
	bind(monaco.KeyCode.LeftArrow | monaco.KeyMod.Shift, "cursorLeftSelect");
	bind(monaco.KeyCode.RightArrow | monaco.KeyMod.Shift, "cursorRightSelect");
	bind(monaco.KeyCode.Tab, "tab");
	bind(monaco.KeyCode.Tab | monaco.KeyMod.Shift, "outdent");

	return [];
}

function installCodeEditorFocusGuards(editor: StandaloneEditor): DisposableLike[] {
	return [
		editor.onMouseDown(() => {
			editor.focus();
		}),
	];
}

export function preWarmFileContents(
	files: ReadonlyArray<{ absolutePath: string; content: string }>,
) {
	for (const file of files) {
		fileContentCache.set(file.absolutePath, file.content);
	}
}

export function syncVirtualFile(path: string, content: string) {
	fileContentCache.set(path, content);
}

async function ensureRuntime(): Promise<MonacoRuntime> {
	if (!runtimePromise) {
		runtimePromise = (async () => {
			const monaco = await import("monaco-editor");

			installMonacoEnvironment();
			configureTypeScriptLanguageService(monaco);
			installEditorTheme(monaco);
			installThemeObserver(monaco);

			return { monaco };
		})();
	}

	return runtimePromise;
}

function configureTypeScriptLanguageService(monaco: MonacoModule) {
	const typescript = monaco.typescript;
	const compilerOptions: Monaco.typescript.CompilerOptions = {
		allowJs: true,
		allowNonTsExtensions: true,
		allowSyntheticDefaultImports: true,
		checkJs: false,
		esModuleInterop: true,
		jsx: typescript.JsxEmit.ReactJSX,
		module: typescript.ModuleKind.ESNext,
		moduleResolution: typescript.ModuleResolutionKind.NodeJs,
		noEmit: true,
		skipLibCheck: true,
		strict: false,
		target: typescript.ScriptTarget.ESNext,
	};
	const diagnosticsOptions: Monaco.typescript.DiagnosticsOptions = {
		noSemanticValidation: true,
		noSyntaxValidation: false,
	};

	typescript.typescriptDefaults.setCompilerOptions(compilerOptions);
	typescript.javascriptDefaults.setCompilerOptions(compilerOptions);
	typescript.typescriptDefaults.setDiagnosticsOptions(diagnosticsOptions);
	typescript.javascriptDefaults.setDiagnosticsOptions(diagnosticsOptions);
}

function installThemeObserver(monaco: MonacoModule) {
	if (
		typeof document === "undefined" ||
		typeof MutationObserver === "undefined"
	) {
		return;
	}

	const syncTheme = () => {
		const nextTheme = detectInitialTheme();
		if (nextTheme === desiredTheme) {
			return;
		}
		desiredTheme = nextTheme;
		monaco.editor.setTheme(themeId(nextTheme));
	};

	const observer = new MutationObserver(syncTheme);
	observer.observe(document.documentElement, {
		attributes: true,
		attributeFilter: ["class"],
	});
	syncTheme();
}

function installMonacoEnvironment() {
	const target = globalThis as typeof globalThis & {
		MonacoEnvironment?: {
			getWorker: (_moduleId: string, label: string) => Worker;
		};
	};

	if (target.MonacoEnvironment) {
		return;
	}

	target.MonacoEnvironment = {
		getWorker(_moduleId, label) {
			switch (label) {
				case "json":
					return new jsonWorker();
				case "css":
				case "scss":
				case "less":
					return new cssWorker();
				case "html":
				case "handlebars":
				case "razor":
					return new htmlWorker();
				case "typescript":
				case "javascript":
					return new tsWorker();
				default:
					return new editorWorker();
			}
		},
	};
}

function installEditorTheme(monaco: MonacoModule) {
	monaco.editor.defineTheme("dcc-editor-dark", {
		base: "vs-dark",
		inherit: true,
		rules: [
			{ token: "comment", foreground: "868584" },
			{ token: "string", foreground: "c9b18f" },
			{ token: "keyword", foreground: "c5a3a8" },
			{ token: "number", foreground: "c6b48a" },
			{ token: "regexp", foreground: "9ea693" },
			{ token: "type.identifier", foreground: "a9b0c6" },
			{ token: "identifier", foreground: "faf9f6" },
			{ token: "delimiter", foreground: "afaeac" },
		],
		colors: {
			"editor.background": "#161514",
			"editor.foreground": "#FAF9F6",
			"editor.lineHighlightBackground": "#1f1e1d",
			"editor.lineHighlightBorder": "#00000000",
			"editor.selectionBackground": "#353534",
			"editor.inactiveSelectionBackground": "#2a2928",
			"editor.wordHighlightBackground": "#35353488",
			"editor.wordHighlightStrongBackground": "#45454588",
			"editorCursor.foreground": "#FAF9F6",
			"editorWhitespace.foreground": "#595755",
			"editorIndentGuide.background1": "#2b2a29",
			"editorIndentGuide.activeBackground1": "#4b4946",
			"editorLineNumber.foreground": "#868584",
			"editorLineNumber.activeForeground": "#FAF9F6",
			"editorGutter.background": "#161514",
			"editorWidget.background": "#1e1d1c",
			"editorWidget.border": "#343332",
			"editorSuggestWidget.background": "#1e1d1c",
			"editorSuggestWidget.border": "#343332",
			"editorHoverWidget.background": "#1e1d1c",
			"editorHoverWidget.border": "#343332",
			"scrollbarSlider.background": "#faf9f626",
			"scrollbarSlider.hoverBackground": "#faf9f640",
			"scrollbarSlider.activeBackground": "#faf9f655",
			"minimap.background": "#161514",
			"diffEditor.insertedLineBackground": "#2ea04318",
			"diffEditor.insertedTextBackground": "#2ea04340",
			"diffEditor.removedLineBackground": "#da363318",
			"diffEditor.removedTextBackground": "#da363340",
			"diffEditorGutter.insertedLineBackground": "#2ea04326",
			"diffEditorGutter.removedLineBackground": "#da363326",
			"diffEditorOverview.insertedForeground": "#2ea04399",
			"diffEditorOverview.removedForeground": "#da363399",
			"diffEditor.diagonalFill": "#faf9f608",
		},
	});

	monaco.editor.defineTheme("dcc-editor-light", {
		base: "vs",
		inherit: true,
		rules: [
			{ token: "comment", foreground: "7a7775" },
			{ token: "string", foreground: "8a6b3d" },
			{ token: "keyword", foreground: "8a3d51" },
			{ token: "number", foreground: "8a6e2f" },
			{ token: "regexp", foreground: "5a6b3d" },
			{ token: "type.identifier", foreground: "3d4d75" },
			{ token: "identifier", foreground: "1a1918" },
			{ token: "delimiter", foreground: "5a5857" },
		],
		colors: {
			"editor.background": "#FFFFFF",
			"editor.foreground": "#1a1918",
			"editor.lineHighlightBackground": "#f4f3f1",
			"editor.lineHighlightBorder": "#00000000",
			"editor.selectionBackground": "#c9d9ef",
			"editor.inactiveSelectionBackground": "#dde3ec",
			"editor.wordHighlightBackground": "#c9d9ef88",
			"editor.wordHighlightStrongBackground": "#a8c1e288",
			"editorCursor.foreground": "#1a1918",
			"editorWhitespace.foreground": "#c7c5c2",
			"editorIndentGuide.background1": "#eceae6",
			"editorIndentGuide.activeBackground1": "#c7c5c2",
			"editorLineNumber.foreground": "#a4a19d",
			"editorLineNumber.activeForeground": "#1a1918",
			"editorGutter.background": "#FFFFFF",
			"editorWidget.background": "#f8f7f5",
			"editorWidget.border": "#e4e2de",
			"editorSuggestWidget.background": "#f8f7f5",
			"editorSuggestWidget.border": "#e4e2de",
			"editorHoverWidget.background": "#f8f7f5",
			"editorHoverWidget.border": "#e4e2de",
			"scrollbarSlider.background": "#1a191826",
			"scrollbarSlider.hoverBackground": "#1a191840",
			"scrollbarSlider.activeBackground": "#1a191855",
			"minimap.background": "#FFFFFF",
			"diffEditor.insertedLineBackground": "#2ea04318",
			"diffEditor.insertedTextBackground": "#2ea04333",
			"diffEditor.removedLineBackground": "#da363318",
			"diffEditor.removedTextBackground": "#da363333",
			"diffEditorGutter.insertedLineBackground": "#2ea04326",
			"diffEditorGutter.removedLineBackground": "#da363326",
			"diffEditorOverview.insertedForeground": "#2ea04399",
			"diffEditorOverview.removedForeground": "#da363399",
			"diffEditor.diagonalFill": "#1a19180a",
		},
	});

	monaco.editor.setTheme(themeId(desiredTheme));
}

function resolveLanguageId(monaco: MonacoModule, path: string): string | undefined {
	const normalizedPath = path.replace(/\\/g, "/");
	const fileName = normalizedPath.split("/").pop()?.toLowerCase() ?? "";
	const extension = fileName.includes(".")
		? fileName.slice(fileName.lastIndexOf("."))
		: "";

	const explicitMap: Record<string, string> = {
		".cjs": "javascript",
		".css": "css",
		".go": "go",
		".html": "html",
		".java": "java",
		".js": "javascript",
		".json": "json",
		".jsx": "javascript",
		".md": "markdown",
		".mjs": "javascript",
		".py": "python",
		".rs": "rust",
		".scss": "scss",
		".sh": "shell",
		".sql": "sql",
		".toml": "ini",
		".ts": "typescript",
		".tsx": "typescript",
		".txt": "plaintext",
		".yaml": "yaml",
		".yml": "yaml",
	};

	if (fileName === "dockerfile") {
		return "dockerfile";
	}

	if (fileName.endsWith(".test.tsx") || fileName.endsWith(".spec.tsx")) {
		return "typescript";
	}

	if (explicitMap[extension]) {
		return explicitMap[extension];
	}

	return monaco.languages.getLanguages().find((language) => {
		const extensions = language.extensions ?? [];
		const filenames = language.filenames ?? [];
		return extensions.includes(extension) || filenames.includes(fileName);
	})?.id;
}

function revealEditorPosition(
	editor: StandaloneEditor,
	line?: number,
	column?: number,
) {
	if (!line) {
		return;
	}

	const position = {
		lineNumber: Math.max(1, line),
		column: Math.max(1, column ?? 1),
	};
	editor.setPosition(position);
	editor.revealPositionInCenter(position);
	focusCodeEditor(editor);
}
