import { $isListItemNode, ListItemNode, ListNode } from "@lexical/list";
import { ORDERED_LIST, UNORDERED_LIST } from "@lexical/markdown";
import { $getSelection, $isRangeSelection, type LexicalNode } from "lexical";

export const COMPOSER_LIST_NODES = [ListNode, ListItemNode];
export const COMPOSER_LIST_TRANSFORMERS = [UNORDERED_LIST, ORDERED_LIST];

export function $isComposerSelectionInList(): boolean {
	const selection = $getSelection();
	if (!$isRangeSelection(selection)) return false;
	let node: LexicalNode | null = selection.anchor.getNode();
	while (node) {
		if ($isListItemNode(node)) return true;
		node = node.getParent();
	}
	return false;
}
