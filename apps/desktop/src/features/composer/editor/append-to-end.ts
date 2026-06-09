import {
	$createParagraphNode,
	$createTextNode,
	$getRoot,
	$isElementNode,
	type ElementNode,
	type LexicalNode,
} from "lexical";

/** Append nodes at the end of the last block; trailing space for caret. */
export function $appendNodesToComposerEnd(...nodes: LexicalNode[]) {
	const root = $getRoot();
	let lastChild = root.getLastChild();
	if (!lastChild || !$isElementNode(lastChild)) {
		lastChild = $createParagraphNode();
		root.append(lastChild);
	}
	const paragraph = lastChild as ElementNode;
	for (const node of nodes) {
		paragraph.append(node);
	}
	const spacer = $createTextNode(" ");
	paragraph.append(spacer);
	spacer.select(1, 1);
}
