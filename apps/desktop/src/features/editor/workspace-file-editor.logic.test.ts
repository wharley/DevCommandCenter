import { describe, expect, it } from "vitest";
import {
	collapsedEditorState,
	documentEndPosition,
	editorPositionToHandle,
	fullDocumentReplacement,
	handlePositionToEditor,
	primaryHandlePosition,
	snippetForOneBasedLines,
} from "./workspace-file-editor.logic";

describe("workspace file editor adapter", () => {
	it("converts Pierre zero-based positions to the one-based handle contract", () => {
		expect(editorPositionToHandle({ line: 4, character: 8 })).toEqual({
			lineNumber: 5,
			column: 9,
		});
		expect(handlePositionToEditor(5, 9)).toEqual({ line: 4, character: 8 });
		expect(handlePositionToEditor(0, -2)).toEqual({ line: 0, character: 0 });
	});

	it("restores a collapsed cursor without changing coordinate systems", () => {
		const state = collapsedEditorState(3, 7);
		expect(primaryHandlePosition(state)).toEqual({ lineNumber: 3, column: 7 });
		expect(state.selections?.[0]).toEqual({
			start: { line: 2, character: 6 },
			end: { line: 2, character: 6 },
			direction: 0,
		});
	});

	it("builds an exact full-document replacement including trailing newlines", () => {
		expect(documentEndPosition("one\ntwo\n")).toEqual({ line: 2, character: 0 });
		expect(fullDocumentReplacement("one\ntwo\n", "next")).toEqual({
			range: {
				start: { line: 0, character: 0 },
				end: { line: 2, character: 0 },
			},
			newText: "next",
		});
		expect(fullDocumentReplacement("same", "same")).toBeNull();
	});

	it("extracts inclusive snippets using the public one-based contract", () => {
		expect(snippetForOneBasedLines("a\nb\nc\nd", 2, 3)).toBe("b\nc");
		expect(snippetForOneBasedLines("a\nb\nc", 3, 2)).toBe("b\nc");
	});
});
