import { useEffect } from "react";
import type { EditorState } from "lexical";
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin";
import { $extractComposerPrompt } from "../extract-composer-prompt";

export function HasContentPlugin({
	onChange,
}: {
	onChange: (hasContent: boolean) => void;
}) {
	const handleChange = (editorState: EditorState) => {
		const text = editorState.read(() => $extractComposerPrompt());
		onChange(text.trim().length > 0);
	};

	useEffect(() => {
		onChange(false);
	}, [onChange]);

	return <OnChangePlugin onChange={handleChange} ignoreSelectionChange />;
}
