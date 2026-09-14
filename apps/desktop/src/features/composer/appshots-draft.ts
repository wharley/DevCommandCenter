import type { LexicalEditor } from "lexical";
import type { PendingAppshot } from "@/lib/appshots-api";
import { pathRelativeToWorkspace } from "@/lib/path-basename";
import { saveDraft } from "./draftStorage";
import { $appendNodesToComposerEnd } from "./editor/append-to-end";
import { $readComposerContext } from "./editor/composer-context";
import { $extractComposerPrompt } from "./editor/extract-composer-prompt";
import { $createImageBadgeNode } from "./editor/image-badge-node";
import { $createFileBadgeNode } from "./editor/file-badge-node";

/** Persist before acknowledging delivery, so a failed/repeated IPC is recoverable. */
export function appendAppshotsToDraft(
	editor: LexicalEditor,
	draftKey: string,
	workspaceRoot: string | null,
	imagesSupported: boolean,
	shots: PendingAppshot[],
): string[] {
	const matching = shots.filter((shot) => shot.draftKey === draftKey);
	if (!matching.length) return [];
	editor.update(
		() => {
			const existing = new Set(
				$readComposerContext()
					.filter((item) => item.kind === "image" || item.kind === "file")
					.map((item) => item.value),
			);
			for (const shot of matching) {
				const path = pathRelativeToWorkspace(workspaceRoot, shot.path);
				if (existing.has(path)) continue;
				$appendNodesToComposerEnd(
					imagesSupported
						? $createImageBadgeNode(path)
						: $createFileBadgeNode(path),
				);
				existing.add(path);
			}
		},
		{ discrete: true },
	);
	const state = editor.getEditorState();
	saveDraft(draftKey, state.read($extractComposerPrompt), state.toJSON());
	return matching.map((shot) => shot.id);
}
