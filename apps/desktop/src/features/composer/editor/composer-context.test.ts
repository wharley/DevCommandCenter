import { beforeEach, describe, expect, it } from "vitest";
import {
	$createParagraphNode,
	$createTextNode,
	$getNodeByKey,
	$getRoot,
	createEditor,
} from "lexical";
import { $createFileBadgeNode, FileBadgeNode } from "./file-badge-node";
import { $createImageBadgeNode, ImageBadgeNode } from "./image-badge-node";
import {
	$createPastedSnippetBadgeNode,
	PastedSnippetBadgeNode,
} from "./pasted-snippet-badge-node";
import { $readComposerContext } from "./composer-context";
import { $extractComposerPrompt } from "./extract-composer-prompt";
import {
	clearDraft,
	loadDraft,
	loadStructuredDraft,
	saveDraft,
} from "../draftStorage";

const create = () =>
	createEditor({
		namespace: "context-test",
		nodes: [FileBadgeNode, ImageBadgeNode, PastedSnippetBadgeNode],
		onError(error) {
			throw error;
		},
	});
const seed = () => {
	const editor = create();
	editor.update(
		() => {
			$getRoot().append(
				$createParagraphNode().append(
					$createTextNode("Review "),
					$createFileBadgeNode("src/config.ts"),
					$createTextNode(" and "),
					$createFileBadgeNode("src/config.ts"),
					$createImageBadgeNode("capture.png"),
					$createPastedSnippetBadgeNode(
						"Keep the existing API.\nAdd keyboard access.",
					),
				),
			);
		},
		{ discrete: true },
	);
	return editor;
};
describe("composer context and structured drafts", () => {
	beforeEach(() => localStorage.clear());
	it("removes only the chosen occurrence without changing surrounding text or other attachments", () => {
		const editor = seed();
		const items = editor.getEditorState().read($readComposerContext);
		expect(items.map((item) => item.kind)).toEqual([
			"file",
			"file",
			"image",
			"snippet",
		]);
		editor.update(
			() => {
				$getNodeByKey(items[0]!.key)?.remove();
			},
			{ discrete: true },
		);
		expect(
			editor
				.getEditorState()
				.read($readComposerContext)
				.map((item) => item.key),
		).toEqual(items.slice(1).map((item) => item.key));
		expect(editor.getEditorState().read($extractComposerPrompt)).toContain(
			"Review  and @src/config.ts",
		);
	});
	it("restores attachment types and exact send text across reload, isolated by draft key", () => {
		const editor = seed();
		const text = editor.getEditorState().read($extractComposerPrompt);
		saveDraft("draft-a", text, editor.getEditorState().toJSON());
		const restored = create();
		restored.setEditorState(
			restored.parseEditorState(loadStructuredDraft("draft-a")!),
		);
		expect(restored.getEditorState().read($extractComposerPrompt)).toBe(text);
		expect(
			restored
				.getEditorState()
				.read($readComposerContext)
				.map((item) => [item.kind, item.value]),
		).toEqual(
			editor
				.getEditorState()
				.read($readComposerContext)
				.map((item) => [item.kind, item.value]),
		);
		expect(loadDraft("draft-b")).toBe("");
		expect(loadStructuredDraft("draft-b")).toBeNull();
	});
	it("plain-text replacement and clearing cannot resurrect old attachment state", () => {
		const editor = seed();
		saveDraft("draft", "old", editor.getEditorState().toJSON());
		saveDraft("draft", "replacement");
		expect(loadStructuredDraft("draft")).toBeNull();
		saveDraft("draft", "old", editor.getEditorState().toJSON());
		clearDraft("draft");
		expect(loadDraft("draft")).toBe("");
		expect(loadStructuredDraft("draft")).toBeNull();
	});
	it("falls back to legacy text when structured data is malformed or stale", () => {
		saveDraft("draft", "legacy text");
		localStorage.setItem("draft.editor.v1", "invalid-json");
		expect(loadStructuredDraft("draft")).toBeNull();
		const editor = seed();
		localStorage.setItem(
			"draft.editor.v1",
			JSON.stringify({
				text: "different",
				state: editor.getEditorState().toJSON(),
			}),
		);
		expect(loadStructuredDraft("draft")).toBeNull();
		expect(loadDraft("draft")).toBe("legacy text");
	});
});
