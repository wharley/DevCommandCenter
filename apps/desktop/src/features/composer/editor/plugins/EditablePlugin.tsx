import { useEffect } from "react";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";

export function EditablePlugin({ disabled }: { disabled: boolean }) {
	const [editor] = useLexicalComposerContext();

	useEffect(() => {
		editor.setEditable(!disabled);
	}, [disabled, editor]);

	return null;
}
