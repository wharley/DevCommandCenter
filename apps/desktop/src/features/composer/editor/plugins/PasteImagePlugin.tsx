import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	$createLineBreakNode,
	$createTextNode,
	$getSelection,
	$isRangeSelection,
	COMMAND_PRIORITY_CRITICAL,
	PASTE_COMMAND,
} from "lexical";
import { useEffect } from "react";
import { isImageFilePath } from "@/lib/is-image-path";
import { pathRelativeToWorkspace } from "@/lib/path-basename";
import { saveClipboardImageToTempFile } from "@/lib/composer-paste";
import { $appendNodesToComposerEnd } from "../append-to-end";
import { $createImageBadgeNode } from "../image-badge-node";
import { $createPastedSnippetBadgeNode } from "../pasted-snippet-badge-node";

/** Collapse huge plain-text pastes into an inline snippet badge (Helmor case 3). */
const LARGE_PASTE_CHAR_THRESHOLD = 1200;

function getClipboardData(event: unknown) {
	if (!event || typeof event !== "object" || !("clipboardData" in event)) {
		return null;
	}
	return (
		(
			event as {
				clipboardData?: {
					files?: File[] | FileList;
					getData?: (format: string) => string;
				} | null;
			}
		).clipboardData ?? null
	);
}

/**
 * Clipboard images → temp file via Tauri (`terminal_save_temp_image`).
 * Plain-text lines that are image paths → `ImageBadgeNode`.
 * Very long plain-text → `PastedSnippetBadgeNode` (full body still serialized on send).
 */
export function PasteImagePlugin({
	workspaceRootPath,
}: {
	workspaceRootPath: string | null;
}) {
	const [editor] = useLexicalComposerContext();

	useEffect(() => {
		return editor.registerCommand(
			PASTE_COMMAND,
			(event) => {
				const clipboardData = getClipboardData(event);
				if (!clipboardData) {
					return false;
				}

				const imageFiles: File[] = [];
				for (const file of Array.from(clipboardData.files ?? [])) {
					if (file.type.startsWith("image/")) {
						imageFiles.push(file);
					}
				}

				if (imageFiles.length > 0) {
					event.preventDefault();

					for (const file of imageFiles) {
						void saveClipboardImageToTempFile(file)
							.then((savedPath) => {
								const stored = pathRelativeToWorkspace(
									workspaceRootPath,
									savedPath,
								);
								editor.update(() => {
									$appendNodesToComposerEnd(
										$createImageBadgeNode(stored),
									);
								});
							})
							.catch((err: unknown) => {
								console.error("[PasteImagePlugin] Failed to save image:", err);
							});
					}

					return true;
				}

				const text = clipboardData.getData?.("text/plain") ?? "";
				if (!text) {
					return false;
				}

				if (text.length >= LARGE_PASTE_CHAR_THRESHOLD) {
					event.preventDefault();
					editor.update(() => {
						const selection = $getSelection();
						const badge = $createPastedSnippetBadgeNode(text);
						if ($isRangeSelection(selection)) {
							selection.insertNodes([badge]);
						} else {
							$appendNodesToComposerEnd(badge);
						}
					});
					return true;
				}

				const lines = text.split("\n");
				const hasImages = lines.some((line) => isImageFilePath(line.trim()));
				if (hasImages) {
					event.preventDefault();

					editor.update(() => {
						const selection = $getSelection();
						if (!$isRangeSelection(selection)) {
							return;
						}

						for (let i = 0; i < lines.length; i++) {
							const trimmed = lines[i].trim();
							if (isImageFilePath(trimmed)) {
								const stored = pathRelativeToWorkspace(
									workspaceRootPath,
									trimmed,
								);
								selection.insertNodes([$createImageBadgeNode(stored)]);
							} else if (trimmed.length > 0) {
								selection.insertNodes([$createTextNode(trimmed)]);
							}
							if (i < lines.length - 1) {
								selection.insertNodes([$createLineBreakNode()]);
							}
						}
					});

					return true;
				}

				return false;
			},
			COMMAND_PRIORITY_CRITICAL,
		);
	}, [editor, workspaceRootPath]);

	return null;
}
