import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	$createParagraphNode,
	$createTextNode,
	$getRoot,
	$isElementNode,
	type ElementNode,
	COMMAND_PRIORITY_CRITICAL,
	DROP_COMMAND,
} from "lexical";
import { useEffect, useRef } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { pathRelativeToWorkspace } from "@/lib/path-basename";
import { $createFileBadgeNode } from "../file-badge-node";
import { $createImageBadgeNode } from "../image-badge-node";
import {
	WORKTREE_FILE_DRAG_TYPE,
	subscribeComposerFileMention,
} from "../../composer-file-mention";

const IMAGE_EXT_RE = /\.(?:png|jpe?g|gif|webp|svg|bmp|ico)$/i;

const DROP_DEDUP_MS = 500;

/** Appends workspace-relative file badges to the draft and puts the caret after them. */
function $appendFileBadges(paths: readonly string[], imagesSupported: boolean) {
	const root = $getRoot();
	let lastChild = root.getLastChild();
	if (!lastChild || !$isElementNode(lastChild)) {
		lastChild = $createParagraphNode();
		root.append(lastChild);
	}
	const paragraph = lastChild as ElementNode;
	for (const path of paths) {
		paragraph.append(
			imagesSupported && IMAGE_EXT_RE.test(path)
				? $createImageBadgeNode(path)
				: $createFileBadgeNode(path),
		);
	}
	const spacer = $createTextNode(" ");
	paragraph.append(spacer);
	spacer.select(1, 1);
}

export function DropFilePlugin({
	workspaceRootPath,
	imagesSupported = true,
}: {
	workspaceRootPath: string | null;
	/** When false, dropped images become plain file references. */
	imagesSupported?: boolean;
}) {
	const [editor] = useLexicalComposerContext();
	const unlistenRef = useRef<(() => void) | null>(null);
	const cancelledRef = useRef(false);
	const lastDropRef = useRef<{ key: string; ts: number }>({ key: "", ts: 0 });

	useEffect(() => {
		cancelledRef.current = false;

		const unregisterDrop = editor.registerCommand(
			DROP_COMMAND,
			(event) => {
				event.preventDefault();
				// A file dragged from the code column's tree becomes a mention.
				const dragged = event.dataTransfer?.getData(WORKTREE_FILE_DRAG_TYPE);
				if (dragged && editor.isEditable()) {
					editor.update(() => $appendFileBadges([dragged], imagesSupported));
					editor.focus();
				}
				return true;
			},
			COMMAND_PRIORITY_CRITICAL,
		);
		// "Mention in chat" from the file tree.
		const unsubscribeMention = subscribeComposerFileMention(({ path }) => {
			if (!editor.isEditable()) return;
			editor.update(() => $appendFileBadges([path], imagesSupported));
			editor.focus();
		});

		unlistenRef.current?.();
		unlistenRef.current = null;

		import("@tauri-apps/api/event")
			.then(({ listen }) => {
				if (cancelledRef.current) {
					return;
				}

				listen<{ paths: string[] }>("tauri://drag-drop", (event) => {
					if (!editor.isEditable()) return;
					const paths = event.payload.paths;
					if (!paths || paths.length === 0) {
						return;
					}

					const key = paths.join("|");
					const now = Date.now();
					if (
						key === lastDropRef.current.key &&
						now - lastDropRef.current.ts < DROP_DEDUP_MS
					) {
						return;
					}
					lastDropRef.current = { key, ts: now };

					editor.update(() => {
						$appendFileBadges(
							paths.map((absolutePath) =>
								pathRelativeToWorkspace(workspaceRootPath, absolutePath),
							),
							imagesSupported,
						);
					});
				}, { target: { kind: "Webview", label: getCurrentWebview().label } }).then((fn) => {
					if (cancelledRef.current) {
						fn();
					} else {
						unlistenRef.current = fn;
					}
				});
			})
			.catch(() => {
				// Non-Tauri web build: drag-drop listener unavailable.
			});

		return () => {
			cancelledRef.current = true;
			unregisterDrop();
			unsubscribeMention();
			unlistenRef.current?.();
			unlistenRef.current = null;
		};
	}, [editor, workspaceRootPath, imagesSupported]);

	return null;
}
