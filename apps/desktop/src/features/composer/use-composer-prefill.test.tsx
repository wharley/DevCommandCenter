import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { PlainTextPlugin } from "@lexical/react/LexicalPlainTextPlugin";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import type { LexicalEditor } from "lexical";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ComposerPrefillPlugin } from "./editor/plugins/ComposerPrefillPlugin";
import { EditorRefPlugin } from "./editor/plugins/EditorRefPlugin";
import { DraftPersistencePlugin } from "./editor/plugins/DraftPersistencePlugin";
import { readComposerPrompt, setEditorText } from "./editorOps";
import { clearDraft } from "./draftStorage";
import {
	useComposerPrefill,
	type ExternalComposerPrefill,
} from "./use-composer-prefill";

const note = { text: "Analisar MCP\n\nRevisar a criação de gateways", nonce: 1 };
const editorRef = { current: null as LexicalEditor | null };
let controller: ReturnType<typeof useComposerPrefill>;
let root: Root;
let container: HTMLDivElement;
const consumed = vi.fn();

function Harness({ draftKey = "new", showEditor = true }: {
	draftKey?: string;
	showEditor?: boolean;
}) {
	const [external, setExternal] = useState<ExternalComposerPrefill | null>(note);
	controller = useComposerPrefill({
		workspaceId: "task-from-note",
		selectedSessionId: null,
		externalComposerPrefill: external,
		onExternalComposerPrefillConsumed: (applied) => {
			consumed(applied);
			setExternal(null);
		},
	});
	return showEditor ? (
		<LexicalComposer initialConfig={{ namespace: "prefill-test", onError: (error) => { throw error; } }}>
			<PlainTextPlugin contentEditable={<ContentEditable />} ErrorBoundary={LexicalErrorBoundary} />
			<EditorRefPlugin editorRef={editorRef} />
			<DraftPersistencePlugin draftKey={draftKey} />
			<ComposerPrefillPlugin key={draftKey} prefill={controller.composerPrefill} onApplied={controller.handleComposerPrefillApplied} />
		</LexicalComposer>
	) : null;
}

beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	consumed.mockClear();
	const createRange = document.createRange.bind(document);
	vi.spyOn(document, "createRange").mockImplementation(() => {
		const range = createRange();
		range.getBoundingClientRect = () => new DOMRect();
		range.getClientRects = () => [] as unknown as DOMRectList;
		return range;
	});
	localStorage.clear();
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});
afterEach(async () => {
	await act(async () => root.unmount());
	container.remove();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

describe("composer prefill lifecycle", () => {
	it("appends diff context while preserving the draft and requests composer focus", async () => {
		await act(async () => root.render(<Harness />));
		await act(async () => setEditorText(editorRef.current!, "Simplifique este teste"));
		const focus = vi.spyOn(editorRef.current!, "focus");
		const context = "Sobre `test.ts` (linhas 19–23):\n\n```\nselected test\n```";
		await act(async () => controller.setComposerPrefill({ requestId: "local:diff", text: context, nonce: 2, mode: "append" }));
		const prompt = readComposerPrompt(editorRef.current!);
		expect(prompt.startsWith("Simplifique este teste")).toBe(true);
		expect(prompt.endsWith(context)).toBe(true);
		expect(focus).toHaveBeenCalled();
		expect(controller.composerPrefill).toBeNull();
	});
	it("keeps the editor empty after sending a note draft and creating the first session", async () => {
		await act(async () => root.render(<Harness />));
		expect(readComposerPrompt(editorRef.current!)).toBe(note.text);
		expect(consumed).toHaveBeenCalledTimes(1);
		await act(async () => {
			clearDraft("new");
			setEditorText(editorRef.current!, "");
		});
		// The effective session changes on first send, remounting the prefill plugin.
		await act(async () => root.render(<Harness draftKey="session-created" />));
		expect(readComposerPrompt(editorRef.current!)).toBe("");
		expect(controller.composerPrefill).toBeNull();
		expect(consumed).toHaveBeenCalledTimes(1);
	});

	it("retains a request until the editor exists and acknowledges the write", async () => {
		await act(async () => root.render(<Harness showEditor={false} />));
		expect(controller.composerPrefill?.text).toBe(note.text);
		expect(consumed).not.toHaveBeenCalled();
		await act(async () => root.render(<Harness />));
		expect(readComposerPrompt(editorRef.current!)).toBe(note.text);
		expect(controller.composerPrefill).toBeNull();
	});

	it("does not consume an external request when a local request has the same text and nonce", async () => {
		await act(async () => root.render(<Harness showEditor={false} />));
		await act(async () => controller.handleComposerPrefillApplied({ ...note, requestId: "local:1" }));
		expect(controller.composerPrefill?.requestId).toBe("external:1");
		expect(consumed).not.toHaveBeenCalled();
	});

	it("does not clear a newer pending request when an older write is acknowledged", async () => {
		await act(async () => root.render(<Harness showEditor={false} />));
		const newer = { text: "Another annotation", nonce: 2, requestId: "local:2" };
		await act(async () => controller.setComposerPrefill(newer));
		await act(async () => controller.handleComposerPrefillApplied({ ...note, requestId: "external:1" }));
		expect(controller.composerPrefill).toEqual(newer);
	});
});
