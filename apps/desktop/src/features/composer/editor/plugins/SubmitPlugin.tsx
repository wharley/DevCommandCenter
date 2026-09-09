import { useEffect } from "react";
import { KEY_ENTER_COMMAND, COMMAND_PRIORITY_HIGH } from "lexical";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";

const TYPEAHEAD_SELECTABLE_SELECTOR = "[data-typeahead-popup] [cmdk-item]";

function isTypeaheadSelectable(): boolean {
	if (typeof document === "undefined") {
		return false;
	}

	return document.querySelector(TYPEAHEAD_SELECTABLE_SELECTOR) !== null;
}

export function SubmitPlugin({
	isDisabled,
	onSubmit,
}: {
	isDisabled: boolean;
	onSubmit: () => void | Promise<void>;
}) {
	const [editor] = useLexicalComposerContext();

	useEffect(() => {
		const submit = () => {
			void onSubmit();
		};

		const unregisterCommand = editor.registerCommand<KeyboardEvent>(
			KEY_ENTER_COMMAND,
			(event) => {
				// Enter on an inline attachment action activates that action, not send.
				if (event?.target instanceof Element && event.target.closest("button"))
					return false;
				if (event?.isComposing || event?.keyCode === 229) {
					return false;
				}

				if (isTypeaheadSelectable()) {
					return false;
				}

				if (event?.shiftKey) {
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
			unregisterCommand();
		};
	}, [editor, isDisabled, onSubmit]);

	return null;
}
