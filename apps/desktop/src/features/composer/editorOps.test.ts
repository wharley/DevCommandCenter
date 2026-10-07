import { describe, expect, it } from "vitest";
import { createEditor } from "lexical";
import { $readComposerContext } from "./editor/composer-context";
import { $extractComposerPrompt } from "./editor/extract-composer-prompt";
import { FileBadgeNode } from "./editor/file-badge-node";
import { ImageBadgeNode } from "./editor/image-badge-node";
import { setEditorPrompt } from "./editorOps";

describe("setEditorPrompt", () => {
	it("puts a sent prompt back with its attachments as badges, so it sends the same again", async () => {
		const editor = createEditor({
			namespace: "edit-from-here",
			nodes: [ImageBadgeNode, FileBadgeNode],
			onError(error) {
				throw error;
			},
		});
		const prompt = "Fix the header @/work/My Shots/header.png and check @/work/src/app.tsx please";
		setEditorPrompt(editor, prompt);
		await Promise.resolve();
		const state = editor.getEditorState();
		expect(state.read($readComposerContext).map((item) => [item.kind, item.value])).toEqual([
			["image", "/work/My Shots/header.png"],
			["file", "/work/src/app.tsx"],
		]);
		expect(state.read($extractComposerPrompt)).toBe(prompt);
	});
});
