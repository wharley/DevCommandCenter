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
import { pathRelativeToWorkspace } from "@/lib/path-basename";
import { $createFileBadgeNode } from "../file-badge-node";
import { $createImageBadgeNode } from "../image-badge-node";

const IMAGE_EXT_RE = /\.(?:png|jpe?g|gif|webp|svg|bmp|ico)$/i;

const DROP_DEDUP_MS = 500;

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
				return true;
			},
			COMMAND_PRIORITY_CRITICAL,
		);

		unlistenRef.current?.();
		unlistenRef.current = null;

		import("@tauri-apps/api/event")
			.then(({ listen }) => {
				if (cancelledRef.current) {
					return;
				}

				listen<{ paths: string[] }>("tauri://drag-drop", (event) => {
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
						const root = $getRoot();
						let lastChild = root.getLastChild();
						if (!lastChild || !$isElementNode(lastChild)) {
							lastChild = $createParagraphNode();
							root.append(lastChild);
						}
						const paragraph = lastChild as ElementNode;

						for (const absolutePath of paths) {
							const stored = pathRelativeToWorkspace(
								workspaceRootPath,
								absolutePath,
							);
							if (imagesSupported && IMAGE_EXT_RE.test(absolutePath)) {
								paragraph.append($createImageBadgeNode(stored));
							} else {
								paragraph.append($createFileBadgeNode(stored));
							}
						}

						const spacer = $createTextNode(" ");
						paragraph.append(spacer);
						spacer.select(1, 1);
					});
				}).then((fn) => {
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
			unlistenRef.current?.();
			unlistenRef.current = null;
		};
	}, [editor, workspaceRootPath]);

	return null;
}
