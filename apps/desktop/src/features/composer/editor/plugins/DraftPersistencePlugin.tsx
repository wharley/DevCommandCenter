import { useEffect } from "react";
import type { EditorState } from "lexical";
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	clearDraft,
	loadDraft,
	loadStructuredDraft,
	saveDraft,
} from "../../draftStorage";
import { setEditorText } from "../../editorOps";
import { $extractComposerPrompt } from "../extract-composer-prompt";
import { $readComposerContext } from "../composer-context";

const EMPTY_FALLBACK_KEYS: readonly string[] = [];

export function DraftPersistencePlugin({
	draftKey,
	fallbackDraftKeys = EMPTY_FALLBACK_KEYS,
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
		const structured = loadStructuredDraft(fallbackKey ?? draftKey);
		if (fallbackKey && stored.trim().length > 0) {
			// Move a legacy/new-session draft exactly once. This prevents it from
			// being replayed into multiple conversations after the first session exists.
			saveDraft(draftKey, stored, structured ?? undefined);
			clearDraft(fallbackKey);
		}
		if (structured && stored.trim()) {
			try {
				const restored = editor.parseEditorState(structured);
				let active = true;
				// Decorator restoration flushes React portals; defer beyond this effect.
				queueMicrotask(() => {
					if (active) editor.setEditorState(restored);
				});
				return () => {
					active = false;
				};
			} catch {
				/* Older or malformed structured drafts retain their plain text. */
			}
		}
		setEditorText(editor, stored);
	}, [draftKey, editor, fallbackDraftKeys]);

	const handleChange = (editorState: EditorState) => {
		const { value, hasContext } = editorState.read(() => ({
			value: $extractComposerPrompt(),
			hasContext: $readComposerContext().length > 0,
		}));
		if (value.trim().length === 0) {
			clearDraft(draftKey);
			return;
		}

		saveDraft(draftKey, value, hasContext ? editorState.toJSON() : undefined);
	};

	return <OnChangePlugin onChange={handleChange} ignoreSelectionChange />;
}
