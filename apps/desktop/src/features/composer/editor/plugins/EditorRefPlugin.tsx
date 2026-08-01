import { useEffect } from "react";
import type { LexicalEditor } from "lexical";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import type { MutableRefObject } from "react";

export function EditorRefPlugin({
	editorRef,
	focusRequestKey = null,
}: {
	editorRef: MutableRefObject<LexicalEditor | null>;
	focusRequestKey?: number | null;
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

	useEffect(() => {
		if (focusRequestKey === null) {
			return;
		}
		// Creation can finish while a Dialog/Popover is still closing. Waiting past
		// its short exit animation prevents the overlay from restoring focus to its
		// trigger after the new task composer has already focused itself.
		const timeout = window.setTimeout(() => editor.focus(), 160);
		return () => window.clearTimeout(timeout);
	}, [editor, focusRequestKey]);

	return null;
}
