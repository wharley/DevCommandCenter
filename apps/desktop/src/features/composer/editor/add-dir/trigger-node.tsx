import {
	$applyNodeReplacement,
	DecoratorNode,
	type DOMExportOutput,
	type LexicalNode,
	type NodeKey,
	type SerializedLexicalNode,
} from "lexical";
import type { ReactNode } from "react";

/** Purple `/add-dir` pill — verbatim chrome from helmor `add-dir/trigger-node.tsx`. */
export class AddDirTriggerNode extends DecoratorNode<ReactNode> {
	static getType(): string {
		return "add-dir-trigger";
	}

	static clone(node: AddDirTriggerNode): AddDirTriggerNode {
		return new AddDirTriggerNode(node.__key);
	}

	static importJSON(_serialized: SerializedLexicalNode): AddDirTriggerNode {
		return $createAddDirTriggerNode();
	}

	constructor(key?: NodeKey) {
		super(key);
	}

	exportJSON(): SerializedLexicalNode {
		return { type: AddDirTriggerNode.getType(), version: 1 };
	}

	createDOM(): HTMLElement {
		const span = document.createElement("span");
		span.style.display = "inline";
		return span;
	}

	updateDOM(): false {
		return false;
	}

	getTextContent(): string {
		return "/add-dir";
	}

	exportDOM(): DOMExportOutput {
		const span = document.createElement("span");
		span.textContent = "/add-dir";
		return { element: span };
	}

	isInline(): true {
		return true;
	}

	decorate(): ReactNode {
		return (
			<span
				data-testid="add-dir-pill"
				className="inline-flex items-center rounded-[4px] px-1.5 py-px font-mono text-[12px] leading-none bg-[color-mix(in_srgb,var(--workspace-pr-merged-accent)_10%,transparent)] text-[var(--workspace-pr-merged-accent)]"
			>
				/add-dir
			</span>
		);
	}
}

export function $createAddDirTriggerNode(): AddDirTriggerNode {
	return $applyNodeReplacement(new AddDirTriggerNode());
}

export function $isAddDirTriggerNode(
	node: LexicalNode | null | undefined,
): node is AddDirTriggerNode {
	return node instanceof AddDirTriggerNode;
}
