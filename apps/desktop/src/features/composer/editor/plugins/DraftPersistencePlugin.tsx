import { useEffect } from "react";
import type { EditorState } from "lexical";
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { clearDraft, loadDraft, saveDraft } from "../../draftStorage";
import { setEditorText } from "../../editorOps";
import { $extractComposerPrompt } from "../extract-composer-prompt";

export function DraftPersistencePlugin({
	draftKey,
	fallbackDraftKeys = [],
}: {
	draftKey: string;
	fallbackDraftKeys?: readonly string[];
}) {
	const [editor] = useLexicalComposerContext();

	useEffect(() => {
		const current = loadDraft(draftKey);
		const fallbackKey =
			current.trim().length === 0
				? fallbackDraftKeys.find(
						(key) => key !== draftKey && loadDraft(key).trim().length > 0,
					)
				: undefined;
		const stored =
			current.trim().length > 0
				? current
				: fallbackKey
					? loadDraft(fallbackKey)
					: "";
		if (fallbackKey && stored.trim().length > 0) {
			// Move a legacy/new-session draft exactly once. This prevents it from
			// being replayed into multiple conversations after the first session exists.
			saveDraft(draftKey, stored);
			clearDraft(fallbackKey);
		}
		setEditorText(editor, stored);
	}, [draftKey, editor, fallbackDraftKeys]);

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
