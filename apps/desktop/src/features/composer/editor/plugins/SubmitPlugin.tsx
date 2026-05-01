import { useEffect } from "react";
import {
	KEY_ENTER_COMMAND,
	COMMAND_PRIORITY_HIGH,
	type LexicalEditor,
} from "lexical";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { clearDraft } from "../../draftStorage";
import { readComposerPrompt, setEditorText } from "../../editorOps";

export function SubmitPlugin({
	draftKey,
	isDisabled,
	onSubmittingChange,
	onSubmit,
	registerSubmit,
}: {
	draftKey: string;
	isDisabled: boolean;
	onSubmittingChange: (isSubmitting: boolean) => void;
	onSubmit: (value: string) => Promise<void>;
	registerSubmit: (submit: (() => void) | null) => void;
}) {
	const [editor] = useLexicalComposerContext();

	useEffect(() => {
		const submit = () => {
			void submitFromEditor(editor, draftKey, onSubmittingChange, onSubmit);
		};

		registerSubmit(submit);

		const unregisterCommand = editor.registerCommand<KeyboardEvent>(
			KEY_ENTER_COMMAND,
			(event) => {
				if (!(event.metaKey || event.ctrlKey)) {
					return false;
				}

				if (isDisabled) {
					return true;
				}

				event.preventDefault();
				submit();
				return true;
			},
			COMMAND_PRIORITY_HIGH,
		);
		return () => {
			registerSubmit(null);
			unregisterCommand();
		};
	}, [draftKey, editor, isDisabled, onSubmittingChange, onSubmit, registerSubmit]);

	return null;
}

async function submitFromEditor(
	editor: LexicalEditor,
	draftKey: string,
	onSubmittingChange: (isSubmitting: boolean) => void,
	onSubmit: (value: string) => Promise<void>,
) {
	const prompt = readComposerPrompt(editor).trim();
	if (prompt.length === 0) {
		return;
	}

	onSubmittingChange(true);
	try {
		await onSubmit(prompt);
		clearDraft(draftKey);
		setEditorText(editor, "");
	} finally {
		onSubmittingChange(false);
	}
}
