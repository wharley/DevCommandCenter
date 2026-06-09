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

let runtimePromise: Promise<MonacoRuntime> | null = null;
const fileContentCache = new Map<string, string>();

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

export async function createFileEditor(options: {
	container: HTMLElement;
	path: string;
	content: string;
	line?: number;
	column?: number;
}): Promise<FileEditorController> {
	const runtime = await ensureRuntime();
	const { monaco } = runtime;
	const language = resolveLanguageId(monaco, options.path);
	const model = monaco.editor.createModel(options.content, language);

	fileContentCache.set(options.path, options.content);

	const editor = monaco.editor.create(options.container, {
		automaticLayout: true,
		bracketPairColorization: { enabled: true },
		fontFamily:
			'"SF Mono","Monaco","Cascadia Mono","Roboto Mono","Menlo",monospace',
		fontLigatures: true,
		fontSize: 13,
		lineHeight: 21,
		minimap: { enabled: false },
		model,
		padding: { top: 14, bottom: 24 },
		renderValidationDecorations: "editable",
		scrollBeyondLastLine: false,
		smoothScrolling: true,
		tabSize: 2,
		theme: themeId(desiredTheme),
		wordWrap: "on",
	});

	revealEditorPosition(editor, options.line, options.column);

	const currentModel = model;

	return {
		editor,
		dispose() {
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
		switchFile(path: string, content?: string, line?: number, column?: number) {
			const resolvedContent = content ?? fileContentCache.get(path);
			if (resolvedContent === undefined) {
				return false;
			}

			currentModel.setValue(resolvedContent);

			const nextLanguage = resolveLanguageId(monaco, path);
			if (nextLanguage && currentModel.getLanguageId() !== nextLanguage) {
				monaco.editor.setModelLanguage(currentModel, nextLanguage);
			}

			fileContentCache.set(path, resolvedContent);
			revealEditorPosition(editor, line, column);
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
	/**
	 * When provided, a floating "send to agent" button is shown over either
	 * side of the diff whenever the reviewer selects a non-empty range.
	 */
	onAnnotate?: (payload: DiffAnnotationPayload) => void;
	/** Label for the annotate button; defaults to a pt-BR string. */
	annotateLabel?: string;
}): Promise<DiffEditorController> {
	const runtime = await ensureRuntime();
	const { monaco } = runtime;
	const language = resolveLanguageId(monaco, options.path);

	const originalUri = monaco.Uri.file(options.path).with({
		query: "dcc-review=original",
	});
	const modifiedUri = monaco.Uri.file(options.path).with({
		query: "dcc-review=modified",
	});
	monaco.editor.getModel(originalUri)?.dispose();
	monaco.editor.getModel(modifiedUri)?.dispose();

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
		theme: themeId(desiredTheme),
	});

	editor.setModel({
		original: originalModel,
		modified: modifiedModel,
	});

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
			for (const disposable of annotateDisposables) {
				disposable.dispose();
			}
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
	};
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
			installEditorTheme(monaco);
			installThemeObserver(monaco);

			return { monaco };
		})();
	}

	return runtimePromise;
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
	editor.focus();
}
