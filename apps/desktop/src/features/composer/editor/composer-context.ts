import { $getRoot, $isElementNode, type LexicalNode } from "lexical";
import { $isFileBadgeNode } from "./file-badge-node";
import { $isImageBadgeNode } from "./image-badge-node";
import { $isPastedSnippetBadgeNode } from "./pasted-snippet-badge-node";

export type ComposerContextItem = {
	key: string;
	kind: "file" | "image" | "snippet";
	value: string;
};

/** Read existing nodes; context review never rewrites the prompt or duplicates attachments. */
export function $readComposerContext(): ComposerContextItem[] {
	const items: ComposerContextItem[] = [];
	const visit = (node: LexicalNode) => {
		if ($isFileBadgeNode(node))
			items.push({
				key: node.getKey(),
				kind: "file",
				value: node.getFilePath(),
			});
		else if ($isImageBadgeNode(node))
			items.push({
				key: node.getKey(),
				kind: "image",
				value: node.getImagePath(),
			});
		else if ($isPastedSnippetBadgeNode(node))
			items.push({
				key: node.getKey(),
				kind: "snippet",
				value: node.getBody(),
			});
		else if ($isElementNode(node)) node.getChildren().forEach(visit);
	};
	visit($getRoot());
	return items;
}
