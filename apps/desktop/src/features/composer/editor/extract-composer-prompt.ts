import {
	$getRoot,
	$isElementNode,
	$isLineBreakNode,
	$isTextNode,
} from "lexical";
import { $isAddDirTriggerNode } from "./add-dir/trigger-node";
import { $isFileBadgeNode } from "./file-badge-node";
import { $isImageBadgeNode } from "./image-badge-node";
import { $isPastedSnippetBadgeNode } from "./pasted-snippet-badge-node";

/**
 * Serializes composer editor state for `send_turn`.
 * `$extractComposerContent` text assembly (badges → `@path`).
 */
export function $extractComposerPrompt(): string {
	const textParts: string[] = [];
	const root = $getRoot();

	for (let pi = 0; pi < root.getChildrenSize(); pi++) {
		const paragraph = root.getChildAtIndex(pi);
		if (!paragraph) {
			continue;
		}

		if (pi > 0) {
			textParts.push("\n");
		}

		if ($isElementNode(paragraph)) {
			for (const child of paragraph.getChildren()) {
				if ($isTextNode(child)) {
					const prev = child.getPreviousSibling();
					if ($isAddDirTriggerNode(prev)) {
						continue;
					}
					textParts.push(child.getTextContent());
				} else if ($isAddDirTriggerNode(child)) {
					continue;
				} else if ($isImageBadgeNode(child)) {
					const path = child.getImagePath();
					const last = textParts[textParts.length - 1];
					if (last && !last.endsWith(" ") && !last.endsWith("\n")) {
						textParts.push(" ");
					}
					textParts.push(`@${path}`);
				} else if ($isFileBadgeNode(child)) {
					const path = child.getFilePath();
					const last = textParts[textParts.length - 1];
					if (last && !last.endsWith(" ") && !last.endsWith("\n")) {
						textParts.push(" ");
					}
					textParts.push(`@${path}`);
				} else if ($isPastedSnippetBadgeNode(child)) {
					const body = child.getBody();
					const last = textParts[textParts.length - 1];
					if (last !== undefined && last.length > 0 && !last.endsWith("\n")) {
						textParts.push("\n");
					}
					textParts.push(body);
				} else if ($isLineBreakNode(child)) {
					textParts.push("\n");
				}
			}
		} else {
			textParts.push(paragraph.getTextContent());
		}
	}

	return textParts
		.join("")
		.replace(/\n{3,}/g, "\n\n")
		.trim();
}
