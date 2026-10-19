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
import type { ReactNode } from "react";
import { ComposerInlineBadge } from "@/components/ComposerInlineBadge";
import { FileMentionLabel } from "@/components/FileMentionLabel";
import { pathBasename } from "@/lib/path-basename";

type SerializedFileBadgeNode = Spread<{ filePath: string }, SerializedLexicalNode>;

function ComposerFileBadge({
	filePath,
	nodeKey,
}: {
	filePath: string;
	nodeKey: NodeKey;
}) {
	const [editor] = useLexicalComposerContext();
	const fileName = pathBasename(filePath);

	return (
		<ComposerInlineBadge
			label={fileName}
			title={filePath}
			onRemove={() => {
				editor.update(() => {
					const node = $getNodeByKey(nodeKey);
					if ($isFileBadgeNode(node)) {
						node.remove();
					}
				});
			}}
		>
			<FileMentionLabel
				path={filePath}
				className="max-w-[260px] self-center text-foreground [&_svg]:text-muted-foreground [&_svg]:opacity-100"
			/>
		</ComposerInlineBadge>
	);
}

export class FileBadgeNode extends DecoratorNode<ReactNode> {
	__filePath: string;

	static getType(): string {
		return "file-badge";
	}

	static importDOM(): DOMConversionMap | null {
		return {
			span: (domNode: HTMLElement) => {
				const filePath = domNode.dataset.dccFileBadgePath;
				if (!filePath) {
					return null;
				}
				return {
					conversion: (): DOMConversionOutput => ({
						node: $createFileBadgeNode(filePath),
					}),
					priority: 2,
				};
			},
		};
	}

	static clone(node: FileBadgeNode): FileBadgeNode {
		return new FileBadgeNode(node.__filePath, node.__key);
	}

	static importJSON(serializedNode: SerializedFileBadgeNode): FileBadgeNode {
		return $createFileBadgeNode(serializedNode.filePath);
	}

	constructor(filePath: string, key?: NodeKey) {
		super(key);
		this.__filePath = filePath;
	}

	exportJSON(): SerializedFileBadgeNode {
		return {
			type: "file-badge",
			version: 1,
			filePath: this.__filePath,
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
		span.dataset.dccFileBadgePath = this.__filePath;
		span.textContent = `@${this.__filePath}`;
		return { element: span };
	}

	isInline(): true {
		return true;
	}

	getTextContent(): string {
		return `@${this.__filePath}`;
	}

	getFilePath(): string {
		return this.__filePath;
	}

	decorate(): ReactNode {
		return <ComposerFileBadge filePath={this.__filePath} nodeKey={this.__key} />;
	}
}

export function $createFileBadgeNode(filePath: string): FileBadgeNode {
	return $applyNodeReplacement(new FileBadgeNode(filePath));
}

export function $isFileBadgeNode(
	node: LexicalNode | null | undefined,
): node is FileBadgeNode {
	return node instanceof FileBadgeNode;
}
