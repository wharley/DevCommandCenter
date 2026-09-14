import { $createListNode, $createListItemNode } from "@lexical/list";
import { COMPOSER_LIST_NODES } from "./composer-lists";
import { describe, expect, it } from "vitest";
import { $createAutoLinkNode, $createLinkNode } from "@lexical/link";
import {
	$createParagraphNode,
	$createTextNode,
	$getRoot,
	createEditor,
} from "lexical";
import {
	COMPOSER_LINK_NODES,
	matchComposerUrl,
	normalizeComposerUrl,
} from "./rich-text";
import { $extractComposerPrompt } from "./extract-composer-prompt";
import { $createFileBadgeNode, FileBadgeNode } from "./file-badge-node";

const create = () =>
	createEditor({
		nodes: [...COMPOSER_LINK_NODES, ...COMPOSER_LIST_NODES, FileBadgeNode],
		onError: (error) => {
			throw error;
		},
	});
describe("rich composer serialization", () => {
	it("preserves raw URLs, custom destinations, formatting, line breaks and attachments", () => {
		const editor = create();
		editor.update(
			() => {
				$getRoot().append(
					$createParagraphNode().append(
						$createTextNode("Review "),
						$createTextNode("this").toggleFormat("bold"),
						$createTextNode(" "),
						$createAutoLinkNode("https://github.com/openai/codex").append(
							$createTextNode("https://github.com/openai/codex"),
						),
						$createTextNode(" and "),
						$createLinkNode("https://example.com/a(b)").append(
							$createTextNode("the [guide]"),
						),
						$createFileBadgeNode("src/app.ts"),
					),
					$createParagraphNode().append(
						$createTextNode("a`b").toggleFormat("code"),
					),
				);
			},
			{ discrete: true },
		);
		const prompt = editor.getEditorState().read($extractComposerPrompt);
		expect(prompt).toBe(
			"Review **this** https://github.com/openai/codex and [the \\[guide\\]](https://example.com/a%28b%29) @src/app.ts\n``a`b``",
		);
		const restored = create();
		restored.setEditorState(
			restored.parseEditorState(editor.getEditorState().toJSON()),
		);
		expect(restored.getEditorState().read($extractComposerPrompt)).toBe(prompt);
	});
	it("does not send destinations for explicitly removed links", () => {
		const editor = create();
		editor.update(
			() =>
				$getRoot().append(
					$createParagraphNode().append(
						$createAutoLinkNode("https://example.com", {
							isUnlinked: true,
						}).append($createTextNode("guide")),
					),
				),
			{ discrete: true },
		);
		expect(editor.getEditorState().read($extractComposerPrompt)).toBe("guide");
	});
	it.each([
		"javascript:alert(1)",
		"data:text/html,test",
		"file:///etc/passwd",
		"https://",
		"not a url",
	])("rejects invalid or unsafe URL %s", (url) =>
		expect(normalizeComposerUrl(url)).toBeNull(),
	);
	it("matches punctuation and balanced URL parentheses without consuming surrounding prose", () => {
		expect(matchComposerUrl("See (https://example.com/a(b)).")?.text).toBe(
			"https://example.com/a(b)",
		);
		expect(matchComposerUrl("See www.github.com/openai/codex!")?.url).toBe(
			"https://www.github.com/openai/codex",
		);
	});
});

it("serializes nested numbered lists, formatted links and attachments without losing boundaries", () => {
	const editor = create();
	editor.update(
		() => {
			$getRoot().append(
				$createParagraphNode().append($createTextNode("Instructions:")),
				$createListNode("number", 9).append(
					$createListItemNode().append(
						$createTextNode("Inspect ").toggleFormat("bold"),
						$createLinkNode("https://example.com/").append(
							$createTextNode("guide"),
						),
						$createFileBadgeNode("src/app.ts"),
					),
					$createListItemNode().append(
						$createListNode("bullet").append(
							$createListItemNode().append($createTextNode("Nested")),
						),
					),
					$createListItemNode().append($createTextNode("Validate")),
				),
				$createParagraphNode().append($createTextNode("Report results.")),
			);
		},
		{ discrete: true },
	);
	expect(editor.getEditorState().read($extractComposerPrompt)).toBe(
		"Instructions:\n\n9. **Inspect** [guide](https://example.com/) @src/app.ts\n   - Nested\n10. Validate\n\nReport results.",
	);
});
