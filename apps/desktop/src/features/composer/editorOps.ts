import { $createParagraphNode, $createTextNode, $getRoot } from "lexical";
import type { EditorState, LexicalEditor } from "lexical";

export function readEditorText(editorState: EditorState) {
	let text = "";
	editorState.read(() => {
		text = $getRoot().getTextContent();
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
