import { describe, expect, it } from "vitest";
import {
	$createParagraphNode,
	$createTextNode,
	$getRoot,
	$getSelection,
	$isRangeSelection,
	$isTextNode,
	createEditor,
	type LexicalEditor,
} from "lexical";
import { $isSlashCommandNode, SlashCommandNode } from "../slash-command-node";
import { $extractComposerPrompt } from "../extract-composer-prompt";
import {
	leadingCommandToken,
	registerSlashCommandHighlight,
} from "./SlashCommandHighlightPlugin";

const editorWith = (commands: readonly string[]) => {
	const editor = createEditor({
		namespace: "slash-highlight-test",
		nodes: [SlashCommandNode],
		onError(error) {
			throw error;
		},
	});
	const unregister = registerSlashCommandHighlight(editor, commands);
	return { editor, unregister };
};

const write = (editor: LexicalEditor, ...paragraphs: string[]) =>
	editor.update(
		() => {
			const root = $getRoot();
			root.clear();
			for (const text of paragraphs) {
				root.append($createParagraphNode().append($createTextNode(text)));
			}
		},
		{ discrete: true },
	);

/** The highlighted text, if any, and the whole prompt. */
const read = (editor: LexicalEditor) =>
	editor.getEditorState().read(() => {
		const highlighted = $getRoot()
			.getAllTextNodes()
			.filter($isSlashCommandNode)
			.map((node) => node.getTextContent());
		return { highlighted, prompt: $extractComposerPrompt() };
	});

describe("leadingCommandToken", () => {
	const commands = new Set(["goal", "compact"]);

	it("takes only a known command at the very start", () => {
		expect(leadingCommandToken("/goal tests pass", commands)).toBe("/goal");
		expect(leadingCommandToken("/goal", commands)).toBe("/goal");
		expect(leadingCommandToken("/compact\nnow", commands)).toBe("/compact");
		expect(leadingCommandToken("/goals", commands)).toBeNull();
		expect(leadingCommandToken("/gaol x", commands)).toBeNull();
		expect(leadingCommandToken(" /goal x", commands)).toBeNull();
		expect(leadingCommandToken("set a /goal", commands)).toBeNull();
		expect(leadingCommandToken("/spec", commands)).toBeNull();
	});
});

describe("registerSlashCommandHighlight", () => {
	it("colors only the command and keeps the prompt text as typed", () => {
		const { editor } = editorWith(["goal"]);
		write(editor, "/goal all tests pass");
		expect(read(editor)).toEqual({
			highlighted: ["/goal"],
			prompt: "/goal all tests pass",
		});
	});

	it("leaves typos, other commands and commands later in the text plain", () => {
		const { editor } = editorWith(["goal"]);
		for (const text of ["/gaol x", "/goals", "/spec", "please /goal x"]) {
			write(editor, text);
			expect(read(editor).highlighted).toEqual([]);
		}
	});

	it("drops the color when the command stops being one", () => {
		const { editor } = editorWith(["goal"]);
		write(editor, "/goal");
		expect(read(editor).highlighted).toEqual(["/goal"]);
		editor.update(
			() => {
				const node = $getRoot().getAllTextNodes()[0];
				node.setTextContent("/goa");
			},
			{ discrete: true },
		);
		expect(read(editor)).toEqual({ highlighted: [], prompt: "/goa" });
	});

	it("follows the provider: a command it does not run loses the color", async () => {
		const first = editorWith(["goal"]);
		write(first.editor, "/goal ship it");
		expect(read(first.editor).highlighted).toEqual(["/goal"]);
		first.unregister();
		registerSlashCommandHighlight(first.editor, ["compact"]);
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(read(first.editor)).toEqual({ highlighted: [], prompt: "/goal ship it" });
	});

	it("follows typing key by key, including after and inside the command", () => {
		const { editor } = editorWith(["goal"]);
		editor.update(
			() => {
				const paragraph = $createParagraphNode();
				$getRoot().clear().append(paragraph);
				paragraph.select();
			},
			{ discrete: true },
		);
		const type = (text: string) => {
			for (const char of text) {
				editor.update(
					() => {
						const selection = $getSelection();
						if ($isRangeSelection(selection)) selection.insertText(char);
					},
					{ discrete: true },
				);
			}
		};
		// Backspace without a DOM: select the character before the caret and
		// remove it.
		const backspace = () =>
			editor.update(
				() => {
					const selection = $getSelection();
					if (!$isRangeSelection(selection)) return;
					const end = selection.anchor.getNode();
					if (!$isTextNode(end)) return;
					const offset = selection.anchor.offset;
					const previous = end.getPreviousSibling();
					const [start, startOffset] =
						offset > 0 || !$isTextNode(previous)
							? [end, offset - 1]
							: [previous, previous.getTextContentSize() - 1];
					selection.setTextNodeRange(start, startOffset, end, offset);
					selection.removeText();
				},
				{ discrete: true },
			);

		type("/goa");
		expect(read(editor).highlighted).toEqual([]);
		type("l");
		expect(read(editor).highlighted).toEqual(["/goal"]);
		type(" ok");
		expect(read(editor)).toEqual({ highlighted: ["/goal"], prompt: "/goal ok" });
		backspace();
		backspace();
		backspace();
		expect(read(editor)).toEqual({ highlighted: ["/goal"], prompt: "/goal" });
		type("s");
		expect(read(editor)).toEqual({ highlighted: [], prompt: "/goals" });
		backspace();
		expect(read(editor)).toEqual({ highlighted: ["/goal"], prompt: "/goal" });
	});

	it("only looks at the first paragraph", () => {
		const { editor } = editorWith(["goal"]);
		write(editor, "context first", "/goal later");
		expect(read(editor).highlighted).toEqual([]);
	});
});
