import { useEffect } from "react";
import type { LexicalEditor } from "lexical";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import type { MutableRefObject } from "react";

export function EditorRefPlugin({
	editorRef,
}: {
	editorRef: MutableRefObject<LexicalEditor | null>;
}) {
	const [editor] = useLexicalComposerContext();

	useEffect(() => {
		editorRef.current = editor;
		return () => {
			if (editorRef.current === editor) {
				editorRef.current = null;
			}
		};
	}, [editor, editorRef]);

	return null;
}
