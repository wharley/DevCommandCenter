import { useEffect } from "react";
import type { EditorState } from "lexical";
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin";
import { readEditorText } from "../../editorOps";

export function HasContentPlugin({
	onChange,
}: {
	onChange: (hasContent: boolean) => void;
}) {
	const handleChange = (editorState: EditorState) => {
		onChange(readEditorText(editorState).trim().length > 0);
	};

	useEffect(() => {
		onChange(false);
	}, [onChange]);

	return <OnChangePlugin onChange={handleChange} ignoreSelectionChange />;
}
