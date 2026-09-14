import { beforeEach, describe, expect, it } from "vitest";
import {
	$createParagraphNode,
	$createTextNode,
	$getRoot,
	createEditor,
} from "lexical";
import { appendAppshotsToDraft } from "./appshots-draft";
import { FileBadgeNode } from "./editor/file-badge-node";
import { ImageBadgeNode } from "./editor/image-badge-node";
import { $readComposerContext } from "./editor/composer-context";
import { $extractComposerPrompt } from "./editor/extract-composer-prompt";
import { loadDraft, loadStructuredDraft } from "./draftStorage";

const makeEditor = () =>
	createEditor({
		namespace: "appshots",
		nodes: [ImageBadgeNode, FileBadgeNode],
		onError(error) {
			throw error;
		},
	});
const shot = { id: "one", draftKey: "a", path: "/data/Appshot-Editor-one.png" };
describe("Appshots delivery to composer drafts", () => {
	beforeEach(() => localStorage.clear());
	it("preserves the prompt, deduplicates retries and persists an image that survives reload", () => {
		const editor = makeEditor();
		editor.update(
			() =>
				$getRoot().append(
					$createParagraphNode().append($createTextNode("Fix this bug")),
				),
			{ discrete: true },
		);
		expect(
			appendAppshotsToDraft(editor, "a", "/project", true, [shot]),
		).toEqual(["one"]);
		appendAppshotsToDraft(editor, "a", "/project", true, [shot]);
		const restored = makeEditor();
		restored.setEditorState(
			restored.parseEditorState(loadStructuredDraft("a")!),
		);
		expect(restored.getEditorState().read($extractComposerPrompt)).toContain(
			"Fix this bug",
		);
		expect(
			restored
				.getEditorState()
				.read($readComposerContext)
				.map((item) => [item.kind, item.value]),
		).toEqual([["image", shot.path]]);
	});
	it("never delivers another task’s capture into the active draft", () => {
		const editor = makeEditor();
		expect(appendAppshotsToDraft(editor, "b", null, true, [shot])).toEqual([]);
		expect(loadDraft("b")).toBe("");
		expect(editor.getEditorState().read($readComposerContext)).toEqual([]);
	});
	it("uses the existing file fallback for a provider without vision", () => {
		const editor = makeEditor();
		appendAppshotsToDraft(editor, "a", "/data", false, [shot]);
		expect(
			editor
				.getEditorState()
				.read($readComposerContext)
				.map((item) => [item.kind, item.value]),
		).toEqual([["file", "Appshot-Editor-one.png"]]);
		expect(loadDraft("a")).toContain("@Appshot-Editor-one.png");
	});
});
