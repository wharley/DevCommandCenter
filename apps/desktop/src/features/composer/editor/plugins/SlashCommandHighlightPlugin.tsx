import { useEffect } from "react";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	$createTextNode,
	$getRoot,
	$getSelection,
	$isParagraphNode,
	$isRangeSelection,
	$isTextNode,
	TextNode,
	type LexicalEditor,
} from "lexical";
import {
	$createSlashCommandNode,
	$isSlashCommandNode,
	SlashCommandNode,
} from "../slash-command-node";

/** `/name` at the very start of the message, followed by a space or nothing. */
const LEADING_COMMAND = /^\/([A-Za-z][\w-]*)(?=\s|$)/;

/** The leading command token when `commands` has it, e.g. `"/goal"`. */
export function leadingCommandToken(text: string, commands: ReadonlySet<string>): string | null {
	const match = LEADING_COMMAND.exec(text);
	return match && commands.has(match[1]) ? match[0] : null;
}

/** Swaps `node` for `replacement` without moving the caret. */
function $replaceKeepingCaret(node: TextNode, replacement: TextNode) {
	const selection = $getSelection();
	const key = node.getKey();
	const anchor =
		$isRangeSelection(selection) && selection.anchor.key === key ? selection.anchor.offset : null;
	const focus =
		$isRangeSelection(selection) && selection.focus.key === key ? selection.focus.offset : null;
	replacement.setFormat(node.getFormat());
	replacement.setStyle(node.getStyle());
	node.replace(replacement);
	if (anchor !== null || focus !== null) {
		const length = replacement.getTextContentSize();
		replacement.select(
			Math.min(anchor ?? focus ?? length, length),
			Math.min(focus ?? anchor ?? length, length),
		);
	}
}

function $normalize(node: TextNode, commands: ReadonlySet<string>) {
	const paragraph = node.getParent();
	const isLead =
		$isParagraphNode(paragraph) &&
		paragraph.is($getRoot().getFirstChild()) &&
		node.is(paragraph.getFirstChild());
	const token = isLead ? leadingCommandToken(paragraph.getTextContent(), commands) : null;

	if ($isSlashCommandNode(node)) {
		if (token === null || node.getTextContent() !== token) {
			$replaceKeepingCaret(node, $createTextNode(node.getTextContent()));
		}
		return;
	}
	if (token === null || !node.isSimpleText()) return;
	const text = node.getTextContent();
	// A command split across formatting stays plain rather than half colored.
	if (!text.startsWith(token)) return;
	const head = text.length > token.length ? node.splitText(token.length)[0] : node;
	if ($isTextNode(head)) {
		$replaceKeepingCaret(head, $createSlashCommandNode(token));
	}
}

/**
 * Keeps the leading slash command colored while `commands` has it. Returns
 * the cleanup. Also re-checks the current text, since the provider (and so
 * its commands) can change under text already typed.
 */
export function registerSlashCommandHighlight(
	editor: LexicalEditor,
	commands: readonly string[],
): () => void {
	const known = new Set(commands);
	const transform = (node: TextNode) => {
		if (!editor.isComposing()) $normalize(node, known);
	};
	const unregisterText = editor.registerNodeTransform(TextNode, transform);
	const unregisterCommand = editor.registerNodeTransform(SlashCommandNode, transform);
	editor.update(() => {
		const first = $getRoot().getFirstChild();
		const lead = $isParagraphNode(first) ? first.getFirstChild() : null;
		if ($isTextNode(lead)) lead.markDirty();
	});
	return () => {
		unregisterText();
		unregisterCommand();
	};
}

/**
 * Colors the leading slash command while it is one the selected provider
 * runs when sent; anything else (a typo, a command of another provider, a
 * DCC menu action typed by hand) stays plain, and that is the hint.
 */
export function SlashCommandHighlightPlugin({ commands }: { commands: readonly string[] }) {
	const [editor] = useLexicalComposerContext();
	useEffect(() => registerSlashCommandHighlight(editor, commands), [commands, editor]);
	return null;
}
