import { $createParagraphNode, $createTextNode, $getRoot } from "lexical";
import type { EditorState, LexicalEditor } from "lexical";

import { isImageFilePath } from "@/lib/is-image-path";
import { promptParts } from "@/lib/prompt-attachments";
import { $extractComposerPrompt } from "./editor/extract-composer-prompt";
import { $createFileBadgeNode } from "./editor/file-badge-node";
import { $createImageBadgeNode } from "./editor/image-badge-node";

export function readEditorText(editorState: EditorState) {
	let text = "";
	editorState.read(() => {
		text = $getRoot().getTextContent();
	});
	return text;
}

/** Full composer serialization (text + `@path` for inline badges). */
export function readComposerPrompt(editor: LexicalEditor) {
	let text = "";
	editor.getEditorState().read(() => {
		text = $extractComposerPrompt();
	});
	return text;
}

export function setEditorText(editor: LexicalEditor, text: string) {
	editor.update(() => {
		const root = $getRoot();
		root.clear();

		if (text.length === 0) {
			return;
		}

		const paragraph = $createParagraphNode();
		paragraph.append($createTextNode(text));
		root.append(paragraph);
		paragraph.selectEnd();
	});
}

/**
 * Replaces the draft with a prompt that was sent before, turning its
 * `@/path` attachments back into badges so they travel again as attachments.
 */
export function setEditorPrompt(editor: LexicalEditor, prompt: string) {
	editor.update(() => {
		const root = $getRoot();
		root.clear();
		if (prompt.length === 0) {
			return;
		}
		const paragraph = $createParagraphNode();
		for (const part of promptParts(prompt)) {
			if (part.kind === "attachment") {
				paragraph.append(
					isImageFilePath(part.path)
						? $createImageBadgeNode(part.path)
						: $createFileBadgeNode(part.path),
				);
			} else if (part.value.length > 0) {
				paragraph.append($createTextNode(part.value));
			}
		}
		root.append(paragraph);
		paragraph.selectEnd();
	});
}

/**
 * Appends text to the composer without clobbering an existing draft. When the
 * draft already has content, a blank paragraph is inserted as a separator so
 * the appended block (e.g. a diff annotation) reads as its own chunk.
 */
export function appendComposerText(editor: LexicalEditor, text: string) {
	if (text.length === 0) {
		return;
	}

	editor.update(() => {
		const root = $getRoot();
		const hasContent = root.getTextContent().trim().length > 0;
		if (hasContent) {
			root.append($createParagraphNode());
		}

		const paragraph = $createParagraphNode();
		paragraph.append($createTextNode(text));
		root.append(paragraph);
		paragraph.selectEnd();
	});
}
