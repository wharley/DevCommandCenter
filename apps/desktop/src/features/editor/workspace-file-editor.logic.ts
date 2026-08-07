import type { EditorState, Position, Range, TextEdit } from "@pierre/diffs";

export type FileEditorPosition = { lineNumber: number; column: number };

export function editorPositionToHandle(
	position: Position | null | undefined,
): FileEditorPosition | null {
	if (!position) return null;
	return {
		lineNumber: Math.max(1, position.line + 1),
		column: Math.max(1, position.character + 1),
	};
}

export function handlePositionToEditor(
	lineNumber: number,
	column: number,
): Position {
	return {
		line: Math.max(0, lineNumber - 1),
		character: Math.max(0, column - 1),
	};
}

export function primaryHandlePosition(state: EditorState): FileEditorPosition | null {
	return editorPositionToHandle(state.selections?.[0]?.end);
}

export function collapsedEditorState(
	lineNumber: number,
	column: number,
): EditorState {
	const position = handlePositionToEditor(lineNumber, column);
	return {
		selections: [{ start: position, end: position, direction: 0 }],
	};
}

export function documentEndPosition(text: string): Position {
	const lines = text.split("\n");
	return {
		line: Math.max(0, lines.length - 1),
		character: lines.at(-1)?.length ?? 0,
	};
}

export function fullDocumentReplacement(
	currentText: string,
	nextText: string,
): TextEdit | null {
	if (currentText === nextText) return null;
	const range: Range = {
		start: { line: 0, character: 0 },
		end: documentEndPosition(currentText),
	};
	return { range, newText: nextText };
}

export function snippetForOneBasedLines(
	text: string,
	startLine: number,
	endLine: number,
): string {
	const start = Math.max(1, Math.min(startLine, endLine));
	const end = Math.max(start, Math.max(startLine, endLine));
	return text.split("\n").slice(start - 1, end).join("\n");
}
