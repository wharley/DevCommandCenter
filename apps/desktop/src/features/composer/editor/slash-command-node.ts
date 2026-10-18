import {
	$applyNodeReplacement,
	TextNode,
	type EditorConfig,
	type LexicalNode,
	type NodeKey,
	type SerializedTextNode,
} from "lexical";

/**
 * A slash command at the start of the message that the selected provider
 * runs when it is sent (`/goal`, `/compact`). Plain text in the prompt; the
 * node only colors it, so the person sees it will run as a command.
 */
export class SlashCommandNode extends TextNode {
	static getType(): string {
		return "slash-command";
	}

	static clone(node: SlashCommandNode): SlashCommandNode {
		return new SlashCommandNode(node.__text, node.__key);
	}

	static importJSON(serializedNode: SerializedTextNode): SlashCommandNode {
		return $createSlashCommandNode(serializedNode.text).updateFromJSON(serializedNode);
	}

	constructor(text = "", key?: NodeKey) {
		super(text, key);
	}

	createDOM(config: EditorConfig): HTMLElement {
		const dom = super.createDOM(config);
		dom.classList.add("composer-slash-command");
		return dom;
	}

	// Typing next to the command starts a new text node, so the command
	// keeps its exact text and the highlight ends where the command does.
	canInsertTextBefore(): boolean {
		return false;
	}

	canInsertTextAfter(): boolean {
		return false;
	}

	isTextEntity(): true {
		return true;
	}
}

export function $createSlashCommandNode(text: string): SlashCommandNode {
	return $applyNodeReplacement(new SlashCommandNode(text));
}

export function $isSlashCommandNode(
	node: LexicalNode | null | undefined,
): node is SlashCommandNode {
	return node instanceof SlashCommandNode;
}
