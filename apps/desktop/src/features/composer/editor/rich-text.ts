import { $isListNode } from "@lexical/list";
import {
	$isAutoLinkNode,
	$isLinkNode,
	AutoLinkNode,
	LinkNode,
} from "@lexical/link";
import type { LinkMatcher } from "@lexical/link";
import { $isElementNode, $isTextNode, type LexicalNode } from "lexical";

export const COMPOSER_LINK_NODES = [LinkNode, AutoLinkNode];
export const COMPOSER_TEXT_THEME = {
	link: "composer-link",
	list: {
		ul: "composer-list composer-list-bullet",
		ol: "composer-list composer-list-number",
		listitem: "composer-list-item",
		nested: { listitem: "composer-list-nested" },
	},
	text: {
		bold: "font-semibold",
		italic: "italic",
		strikethrough: "line-through",
		underline: "underline",
		code: "composer-inline-code",
	},
};

export function normalizeComposerUrl(value: string): string | null {
	const input = value.trim();
	if (!input || /\s/.test(input)) return null;
	try {
		const url = new URL(/^www\./i.test(input) ? `https://${input}` : input);
		return ["http:", "https:"].includes(url.protocol) && url.hostname
			? url.href
			: null;
	} catch {
		return null;
	}
}

export const matchComposerUrl: LinkMatcher = (text) => {
	const match = /(?:https?:\/\/|www\.)[^\s<>]+/i.exec(text);
	if (!match) return null;
	let value = match[0].replace(/[.,!?;:]+$/, "");
	// Keep balanced parentheses in paths, leave surrounding prose outside the link.
	while (
		value.endsWith(")") &&
		(value.match(/\)/g)?.length ?? 0) > (value.match(/\(/g)?.length ?? 0)
	)
		value = value.slice(0, -1);
	const url = normalizeComposerUrl(value);
	return url
		? { index: match.index, length: value.length, text: value, url }
		: null;
};
export const COMPOSER_LINK_MATCHERS = [matchComposerUrl];

export function $hasComposerRichText(node: LexicalNode): boolean {
	return (
		$isLinkNode(node) ||
		$isListNode(node) ||
		($isTextNode(node) && node.getFormat() !== 0) ||
		($isElementNode(node) && node.getChildren().some($hasComposerRichText))
	);
}

export function $serializeFormattedText(node: LexicalNode): string {
	let text = node.getTextContent();
	if (!$isTextNode(node)) return text;
	if (node.hasFormat("code")) {
		const fence = "`".repeat(
			Math.max(
				0,
				...Array.from(text.matchAll(/`+/g), (match) => match[0].length),
			) + 1,
		);
		const pad = text.startsWith("`") || text.endsWith("`") ? " " : "";
		return `${fence}${pad}${text}${pad}${fence}`;
	}
	// Keep spaces outside emphasis so the resulting Markdown renders correctly.
	return text.replace(/^(\s*)([\s\S]*?)(\s*)$/, (_, start, body, end) => {
		if (!body) return text;
		if (node.hasFormat("bold")) body = `**${body}**`;
		if (node.hasFormat("italic")) body = `*${body}*`;
		if (node.hasFormat("strikethrough")) body = `~~${body}~~`;
		return start + body + end;
	});
}

export function $serializeComposerLink(node: LinkNode, label: string): string {
	if ($isAutoLinkNode(node) && node.getIsUnlinked()) return label;
	const url = node.getURL();
	if (label === url || normalizeComposerUrl(label) === url) return label;
	return `[${label.replace(/[\[\]\\]/g, "\\$&")}](${url.replace(/[()\s<>]/g, (char) => encodeURIComponent(char).replace("(", "%28").replace(")", "%29"))})`;
}
