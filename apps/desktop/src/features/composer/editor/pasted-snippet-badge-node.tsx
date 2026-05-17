import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	$applyNodeReplacement,
	$getNodeByKey,
	DecoratorNode,
	type DOMConversionMap,
	type DOMConversionOutput,
	type DOMExportOutput,
	type LexicalNode,
	type NodeKey,
	type SerializedLexicalNode,
	type Spread,
} from "lexical";
import { Braces } from "lucide-react";
import type { ReactNode } from "react";
import { ComposerInlineBadge } from "@/components/ComposerInlineBadge";

type SerializedPastedSnippetBadgeNode = Spread<
	{ body: string },
	SerializedLexicalNode
>;

function snippetUiLabel(body: string): string {
	const trimmed = body.trimStart();
	const firstLine = trimmed.split(/\r?\n/)[0] ?? "";
	const preview =
		firstLine.length > 44 ? `${firstLine.slice(0, 44)}…` : firstLine;
	if (preview.length > 0) {
		return `${preview} · ${body.length.toLocaleString()} chars`;
	}
	return `Pasted snippet · ${body.length.toLocaleString()} chars`;
}

function ComposerPastedSnippetBadge({
	body,
	nodeKey,
}: {
	body: string;
	nodeKey: NodeKey;
}) {
	const [editor] = useLexicalComposerContext();

	return (
		<ComposerInlineBadge
			icon={
				<Braces className="size-3.5 shrink-0 text-chart-2" strokeWidth={1.8} />
			}
			label={snippetUiLabel(body)}
			onRemove={() => {
				editor.update(() => {
					const node = $getNodeByKey(nodeKey);
					if ($isPastedSnippetBadgeNode(node)) {
						node.remove();
					}
				});
			}}
		/>
	);
}

export class PastedSnippetBadgeNode extends DecoratorNode<ReactNode> {
	__body: string;

	static getType(): string {
		return "pasted-snippet";
	}

	static importDOM(): DOMConversionMap | null {
		return {
			span: (domNode: HTMLElement) => {
				if (domNode.dataset.dccPastedSnippet !== "true") {
					return null;
				}
				return {
					conversion: (element): DOMConversionOutput => ({
						node: $createPastedSnippetBadgeNode(element.textContent ?? ""),
					}),
					priority: 2,
				};
			},
		};
	}

	static clone(node: PastedSnippetBadgeNode): PastedSnippetBadgeNode {
		return new PastedSnippetBadgeNode(node.__body, node.__key);
	}

	static importJSON(
		serializedNode: SerializedPastedSnippetBadgeNode,
	): PastedSnippetBadgeNode {
		return $createPastedSnippetBadgeNode(serializedNode.body);
	}

	constructor(body: string, key?: NodeKey) {
		super(key);
		this.__body = body;
	}

	exportJSON(): SerializedPastedSnippetBadgeNode {
		return {
			type: "pasted-snippet",
			version: 1,
			body: this.__body,
		};
	}

	createDOM(): HTMLElement {
		const span = document.createElement("span");
		span.style.display = "inline";
		return span;
	}

	updateDOM(): false {
		return false;
	}

	exportDOM(): DOMExportOutput {
		const span = document.createElement("span");
		span.dataset.dccPastedSnippet = "true";
		span.textContent = this.__body;
		return { element: span };
	}

	isInline(): true {
		return true;
	}

	getTextContent(): string {
		return this.__body;
	}

	getBody(): string {
		return this.__body;
	}

	decorate(): ReactNode {
		return (
			<ComposerPastedSnippetBadge body={this.__body} nodeKey={this.__key} />
		);
	}
}

export function $createPastedSnippetBadgeNode(body: string): PastedSnippetBadgeNode {
	return $applyNodeReplacement(new PastedSnippetBadgeNode(body));
}

export function $isPastedSnippetBadgeNode(
	node: LexicalNode | null | undefined,
): node is PastedSnippetBadgeNode {
	return node instanceof PastedSnippetBadgeNode;
}
