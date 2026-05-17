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
import { ImageIcon } from "lucide-react";
import type { ReactNode } from "react";
import { ComposerInlineBadge } from "@/components/ComposerInlineBadge";
import { pathBasename } from "@/lib/path-basename";

type SerializedImageBadgeNode = Spread<{ imagePath: string }, SerializedLexicalNode>;

function ComposerImageBadge({
	imagePath,
	nodeKey,
}: {
	imagePath: string;
	nodeKey: NodeKey;
}) {
	const [editor] = useLexicalComposerContext();
	const fileName = pathBasename(imagePath);

	return (
		<ComposerInlineBadge
			icon={
				<ImageIcon className="size-3.5 shrink-0 text-chart-3" strokeWidth={1.8} />
			}
			label={fileName}
			onRemove={() => {
				editor.update(() => {
					const node = $getNodeByKey(nodeKey);
					if ($isImageBadgeNode(node)) {
						node.remove();
					}
				});
			}}
		/>
	);
}

export class ImageBadgeNode extends DecoratorNode<ReactNode> {
	__imagePath: string;

	static getType(): string {
		return "image-badge";
	}

	static importDOM(): DOMConversionMap | null {
		return {
			span: (domNode: HTMLElement) => {
				const imagePath = domNode.dataset.dccImageBadgePath;
				if (!imagePath) {
					return null;
				}
				return {
					conversion: (): DOMConversionOutput => ({
						node: $createImageBadgeNode(imagePath),
					}),
					priority: 2,
				};
			},
		};
	}

	static clone(node: ImageBadgeNode): ImageBadgeNode {
		return new ImageBadgeNode(node.__imagePath, node.__key);
	}

	static importJSON(serializedNode: SerializedImageBadgeNode): ImageBadgeNode {
		return $createImageBadgeNode(serializedNode.imagePath);
	}

	constructor(imagePath: string, key?: NodeKey) {
		super(key);
		this.__imagePath = imagePath;
	}

	exportJSON(): SerializedImageBadgeNode {
		return {
			type: "image-badge",
			version: 1,
			imagePath: this.__imagePath,
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
		span.dataset.dccImageBadgePath = this.__imagePath;
		span.textContent = `@${this.__imagePath}`;
		return { element: span };
	}

	isInline(): true {
		return true;
	}

	getTextContent(): string {
		return `@${this.__imagePath}`;
	}

	getImagePath(): string {
		return this.__imagePath;
	}

	decorate(): ReactNode {
		return <ComposerImageBadge imagePath={this.__imagePath} nodeKey={this.__key} />;
	}
}

export function $createImageBadgeNode(imagePath: string): ImageBadgeNode {
	return $applyNodeReplacement(new ImageBadgeNode(imagePath));
}

export function $isImageBadgeNode(
	node: LexicalNode | null | undefined,
): node is ImageBadgeNode {
	return node instanceof ImageBadgeNode;
}
