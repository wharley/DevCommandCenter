import { $isLinkNode } from "@lexical/link";
import { $isListItemNode, $isListNode, type ListNode } from "@lexical/list";
import { $serializeComposerLink, $serializeFormattedText } from "./rich-text";
import {
	$getRoot,
	$isElementNode,
	$isLineBreakNode,
	$isTextNode,
	type LexicalNode,
} from "lexical";
import { $isFileBadgeNode } from "./file-badge-node";
import { $isImageBadgeNode } from "./image-badge-node";
import { $isPastedSnippetBadgeNode } from "./pasted-snippet-badge-node";

function $serializeInline(nodes: LexicalNode[]): string {
	const parts: string[] = [];
	const visit = (node: LexicalNode) => {
		if ($isLinkNode(node)) {
			parts.push(
				$serializeComposerLink(node, $serializeInline(node.getChildren())),
			);
		} else if ($isTextNode(node)) {
			parts.push($serializeFormattedText(node));
		} else if ($isImageBadgeNode(node) || $isFileBadgeNode(node)) {
			const path = $isImageBadgeNode(node)
				? node.getImagePath()
				: node.getFilePath();
			const last = parts[parts.length - 1];
			if (last && !last.endsWith(" ") && !last.endsWith("\n")) parts.push(" ");
			parts.push(`@${path}`);
		} else if ($isPastedSnippetBadgeNode(node)) {
			const last = parts[parts.length - 1];
			if (last && !last.endsWith("\n")) parts.push("\n");
			parts.push(node.getBody());
		} else if ($isLineBreakNode(node)) {
			parts.push("\n");
		} else if ($isElementNode(node)) {
			node.getChildren().forEach(visit);
		} else {
			parts.push(node.getTextContent());
		}
	};
	nodes.forEach(visit);
	return parts.join("");
}

function $serializeList(list: ListNode, indent = ""): string {
	const lines: string[] = [];
	let number = list.getStart();
	let nestedIndent = `${indent}  `;
	for (const item of list.getChildren()) {
		if (!$isListItemNode(item)) continue;
		const children = item.getChildren();
		const content = children.filter((child) => !$isListNode(child));
		// Lexical represents indentation with an extra list item containing only
		// a nested list. That wrapper must not emit a marker or consume a number.
		if (content.length > 0 || children.length === 0) {
			const marker = list.getListType() === "number" ? `${number++}. ` : "- ";
			nestedIndent = indent + " ".repeat(marker.length);
			const body = $serializeInline(content).split("\n");
			lines.push(indent + marker + (body[0] ?? ""));
			lines.push(...body.slice(1).map((line) => nestedIndent + line));
		}
		for (const child of children) {
			if ($isListNode(child)) lines.push($serializeList(child, nestedIndent));
		}
	}
	return lines.join("\n");
}

/** Markdown for send_turn, with inline attachment nodes preserved as @path. */
export function $extractComposerPrompt(): string {
	const blocks = $getRoot().getChildren();
	return blocks
		.map((block, index) => {
			const previous = blocks[index - 1];
			// Separate lists from surrounding paragraphs, so following prose does not
			// become a continuation of the final list item in Markdown.
			const separator =
				index === 0
					? ""
					: $isListNode(block) || $isListNode(previous)
						? "\n\n"
						: "\n";
			return (
				separator +
				($isListNode(block) ? $serializeList(block) : $serializeInline([block]))
			);
		})
		.join("")
		.replace(/\n{3,}/g, "\n\n")
		.trim();
}
