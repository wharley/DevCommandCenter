import { useEffect, useRef } from "react";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { appendComposerText, setEditorText } from "../../editorOps";
import { shouldApplyComposerPrefill } from "../../WorkspaceComposer.logic";

import type {
	ComposerPrefill,
	ComposerPrefillConsumption,
} from "../../use-composer-prefill";

/**
 * Applies an external draft injection from inside Lexical's lifecycle.
 *
 * Keeping this in a plugin guarantees that the editor exists before a prefill is
 * acknowledged. The request owner can therefore retain the request across
 * composer/session remounts until `onApplied` confirms the actual write.
 */
export function ComposerPrefillPlugin({
	prefill,
	onApplied,
}: {
	prefill?: ComposerPrefill | null;
	onApplied?: (prefill: ComposerPrefillConsumption) => void;
}) {
	const [editor] = useLexicalComposerContext();
	const lastAppliedRequestIdRef = useRef<string | null>(null);

	useEffect(() => {
		if (!shouldApplyComposerPrefill(lastAppliedRequestIdRef.current, prefill)) {
			return;
		}
		if (!prefill) return;

		if (prefill.mode === "replace") {
			setEditorText(editor, prefill.text);
		} else {
			appendComposerText(editor, prefill.text);
		}
		lastAppliedRequestIdRef.current = prefill.requestId;
		editor.focus();
		onApplied?.(prefill);
	}, [editor, onApplied, prefill]);

	return null;
}
