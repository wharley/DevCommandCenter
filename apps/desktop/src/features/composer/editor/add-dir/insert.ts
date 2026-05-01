import {
	$createParagraphNode,
	$createTextNode,
	$getRoot,
	$isElementNode,
	type ElementNode,
	type LexicalEditor,
	type TextNode,
} from "lexical";
import { $createAddDirTriggerNode } from "./trigger-node";

export function $insertAddDirTrigger(
	editor: LexicalEditor,
	nodeToReplace: TextNode | null,
) {
	editor.update(() => {
		const pill = $createAddDirTriggerNode();
		const trailing = $createTextNode(" ");
		if (nodeToReplace) {
			nodeToReplace.replace(pill);
		} else {
			const root = $getRoot();
			let last = root.getLastChild();
			if (!last || !$isElementNode(last)) {
				last = $createParagraphNode();
				root.append(last);
			}
			(last as ElementNode).append(pill);
		}
		pill.insertAfter(trailing);
		trailing.select(1, 1);
	});
}
