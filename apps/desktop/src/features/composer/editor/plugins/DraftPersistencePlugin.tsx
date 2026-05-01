import { useEffect } from "react";
import type { EditorState } from "lexical";
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { clearDraft, loadDraft, saveDraft } from "../../draftStorage";
import { setEditorText } from "../../editorOps";
import { $extractComposerPrompt } from "../extract-composer-prompt";

export function DraftPersistencePlugin({
	draftKey,
}: {
	draftKey: string;
}) {
	const [editor] = useLexicalComposerContext();

	useEffect(() => {
		const stored = loadDraft(draftKey);
		setEditorText(editor, stored);
	}, [draftKey, editor]);

	const handleChange = (editorState: EditorState) => {
		const value = editorState.read(() => $extractComposerPrompt());
		if (value.trim().length === 0) {
			clearDraft(draftKey);
			return;
		}

		saveDraft(draftKey, value);
	};

	return <OnChangePlugin onChange={handleChange} ignoreSelectionChange />;
}
