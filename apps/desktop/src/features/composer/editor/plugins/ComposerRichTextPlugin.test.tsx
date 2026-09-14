import { SubmitPlugin } from "./SubmitPlugin";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { COMPOSER_LIST_NODES } from "../composer-lists";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { RichTextPlugin } from "@lexical/react/LexicalRichTextPlugin";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import {
	$createParagraphNode,
	$createTextNode,
	$getRoot,
	$getSelection,
	$isRangeSelection,
	KEY_ENTER_COMMAND,
	KEY_MODIFIER_COMMAND,
	UNDO_COMMAND,
	$isTextNode,
	type LexicalEditor,
} from "lexical";
import { ComposerRichTextPlugin } from "./ComposerRichTextPlugin";
import { COMPOSER_LINK_NODES } from "../rich-text";
import { EditorRefPlugin } from "./EditorRefPlugin";
import { DraftPersistencePlugin } from "./DraftPersistencePlugin";
import { loadDraft, loadStructuredDraft } from "../../draftStorage";
import { readComposerPrompt, setEditorText } from "../../editorOps";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key.split(".").pop() }),
}));
const onSubmit = vi.fn();
let root: Root;
let container: HTMLDivElement;
const editorRef: { current: LexicalEditor | null } = { current: null };
const render = (key = "rich-test") =>
	root.render(
		<LexicalComposer
			initialConfig={{
				namespace: "rich-test",
				nodes: [...COMPOSER_LINK_NODES, ...COMPOSER_LIST_NODES],
				onError: (error) => {
					throw error;
				},
			}}
		>
			<RichTextPlugin
				contentEditable={<ContentEditable />}
				ErrorBoundary={LexicalErrorBoundary}
			/>
			<ComposerRichTextPlugin draftKey={key} />
			<SubmitPlugin isDisabled={false} onSubmit={onSubmit} />
			<HistoryPlugin />
			<EditorRefPlugin editorRef={editorRef} />
			<DraftPersistencePlugin draftKey={key} />
		</LexicalComposer>,
	);
const click = async (name: string) => {
	const button = Array.from(document.querySelectorAll("button")).find(
		(button) =>
			button.getAttribute("aria-label") === name || button.textContent === name,
	);
	expect(button, name).toBeTruthy();
	await act(async () => button!.click());
	// Radix restores focus on the next task after a popover unmount.
	await act(async () => new Promise((resolve) => setTimeout(resolve, 0)));
};
const fill = async (index: number, text: string) => {
	await act(async () => {
		const input = document.querySelectorAll("input")[index]!;
		Object.getOwnPropertyDescriptor(
			HTMLInputElement.prototype,
			"value",
		)!.set!.call(input, text);
		input.dispatchEvent(new Event("input", { bubbles: true }));
	});
};
const seed = async (text: string) =>
	act(async () => setEditorText(editorRef.current!, text));
beforeEach(async () => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	vi.stubGlobal(
		"ResizeObserver",
		class {
			observe() {}
			unobserve() {}
			disconnect() {}
		},
	);
	Object.defineProperty(Range.prototype, "getBoundingClientRect", {
		configurable: true,
		value: () => document.body.getBoundingClientRect(),
	});
	Object.defineProperty(Range.prototype, "getClientRects", {
		configurable: true,
		value: () => [],
	});
	onSubmit.mockClear();
	localStorage.clear();
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
	await act(async () => render());
});
afterEach(async () => {
	await act(async () => root.unmount());
	container.remove();
	vi.unstubAllGlobals();
	Reflect.deleteProperty(Range.prototype, "getBoundingClientRect");
	Reflect.deleteProperty(Range.prototype, "getClientRects");
});
it("edits autolink text and destination independently, then removes the link", async () => {
	await seed("https://github.com/openai/codex");
	await act(async () => container.querySelector("a")!.click());
	await click("editText");
	await fill(0, "Codex repository");
	await click("save");
	expect(readComposerPrompt(editorRef.current!)).toBe(
		"[Codex repository](https://github.com/openai/codex)",
	);
	await act(async () => container.querySelector("a")!.click());
	await click("editLink");
	await fill(1, "https://github.com/openai/codex/issues");
	await click("save");
	expect(readComposerPrompt(editorRef.current!)).toBe(
		"[Codex repository](https://github.com/openai/codex/issues)",
	);
	expect(loadStructuredDraft("rich-test")).not.toBeNull();
	await act(async () => container.querySelector("a")!.click());
	await click("removeLink");
	expect(readComposerPrompt(editorRef.current!)).toBe("Codex repository");
	expect(container.querySelector("a")).toBeNull();
});
it("keeps a removed URL unlinked when typing resumes", async () => {
	await seed("https://example.com");
	await act(async () => container.querySelector("a")!.click());
	await click("removeLink");
	expect(container.querySelector("a")).toBeNull();
	expect(readComposerPrompt(editorRef.current!)).toBe("https://example.com");
});
it("inserts into an empty editor and rejects unsafe link edits", async () => {
	await click("insertLink");
	await fill(0, "Documentation");
	await fill(1, "javascript:alert(1)");
	await click("save");
	expect(document.querySelector('[role="alert"]')?.textContent).toBe(
		"invalidUrl",
	);
	expect(readComposerPrompt(editorRef.current!)).toBe("");
	await fill(1, "https://example.com");
	await click("save");
	expect(readComposerPrompt(editorRef.current!)).toBe(
		"[Documentation](https://example.com/)",
	);
});
it("formats selected text and restores it on conversation changes", async () => {
	await act(async () =>
		editorRef.current!.update(() => {
			const text = $createTextNode("Important");
			$getRoot().append($createParagraphNode().append(text));
			text.select(0, 9);
		}),
	);
	await click("bold");
	expect(readComposerPrompt(editorRef.current!)).toBe("**Important**");
	expect(loadDraft("rich-test")).toBe("**Important**");
	await act(async () => render("another-conversation"));
	expect(readComposerPrompt(editorRef.current!)).toBe("");
	await act(async () => render());
	expect(readComposerPrompt(editorRef.current!)).toBe("**Important**");
	expect(
		editorRef.current!.getEditorState().read(() => {
			const text = $getRoot().getFirstDescendant();
			return $isTextNode(text) && text.hasFormat("bold");
		}),
	).toBe(true);
});

const typeText = async (text: string) => {
	for (const char of text) {
		await act(async () =>
			editorRef.current!.update(() => {
				const selection = $getSelection();
				if ($isRangeSelection(selection)) selection.insertText(char);
			}),
		);
	}
};
const enter = async (options: KeyboardEventInit = {}) =>
	act(async () => {
		editorRef.current!.dispatchCommand(
			KEY_ENTER_COMMAND,
			new KeyboardEvent("keydown", {
				key: "Enter",
				cancelable: true,
				...options,
			}),
		);
	});
it.each(["-", "*", "+", "1.", "7."])(
	"turns %s plus space into a list and preserves its Markdown",
	async (marker) => {
		await seed(marker);
		await typeText(" ");
		expect(
			container.querySelector(marker.endsWith(".") ? "ol" : "ul"),
		).not.toBeNull();
		await typeText("First");
		await enter();
		await typeText("Second");
		const start = marker === "7." ? 7 : 1;
		const expected = marker.endsWith(".")
			? `${start}. First\n${start + 1}. Second`
			: "- First\n- Second";
		expect(readComposerPrompt(editorRef.current!)).toBe(expected);
		expect(onSubmit).not.toHaveBeenCalled();
		expect(loadStructuredDraft("rich-test")).not.toBeNull();
		await act(async () => render("another-conversation"));
		await act(async () => render());
		expect(readComposerPrompt(editorRef.current!)).toBe(expected);
		expect(container.querySelectorAll("li")).toHaveLength(2);
	},
);
it("continues lists, exits an empty item, preserves soft breaks and sends only on explicit submission", async () => {
	await seed("-");
	await typeText(" First");
	await enter({ shiftKey: true });
	await typeText("continuation");
	expect(readComposerPrompt(editorRef.current!)).toBe(
		"- First\n  continuation",
	);
	await enter({ metaKey: true });
	expect(onSubmit).toHaveBeenCalledTimes(1);
	await enter();
	await typeText("Second");
	await enter();
	await enter();
	expect(container.querySelectorAll("li")).toHaveLength(2);
	await typeText("After list");
	expect(readComposerPrompt(editorRef.current!)).toBe(
		"- First\n  continuation\n- Second\n\nAfter list",
	);
	await enter();
	expect(onSubmit).toHaveBeenCalledTimes(2);
});
it("undoes automatic list conversion", async () => {
	await seed("-");
	await typeText(" ");
	expect(container.querySelector("ul")).not.toBeNull();
	await act(async () => {
		editorRef.current!.dispatchCommand(UNDO_COMMAND, undefined);
	});
	expect(container.querySelector("ul")).toBeNull();
	expect(readComposerPrompt(editorRef.current!)).toBe("-");
});
it("does not intercept the global search shortcut", async () => {
	await seed("Search");
	const event = new KeyboardEvent("keydown", {
		key: "k",
		metaKey: true,
		cancelable: true,
	});
	await act(async () => {
		editorRef.current!.dispatchCommand(KEY_MODIFIER_COMMAND, event);
	});
	expect(event.defaultPrevented).toBe(false);
	expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("starts an automatic list after prose and keeps inline hyphens as text", async () => {
	await seed("Please review - this");
	await enter({ shiftKey: true });
	await typeText("- First");
	expect(container.querySelector("ul")).not.toBeNull();
	expect(readComposerPrompt(editorRef.current!)).toBe(
		"Please review - this\n\n- First",
	);
	expect(onSubmit).not.toHaveBeenCalled();
});
