import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import type { LexicalEditor } from "lexical";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { AppshotsPlugin } from "./AppshotsPlugin";
import { ImageBadgeNode } from "../image-badge-node";
import { FileBadgeNode } from "../file-badge-node";
import { $readComposerContext } from "../composer-context";
import { loadDraft } from "../../draftStorage";
import { toast } from "sonner";

vi.mock("sonner", () => ({ toast: { error: vi.fn(), info: vi.fn() } }));

vi.mock("react-i18next", () => {
	const t = (key: string) => key;
	return { useTranslation: () => ({ t }) };
});
let root: Root;
let host: HTMLDivElement;
let editor: LexicalEditor;
const ipc = vi.fn();
function Harness({ draftKey }: { draftKey: string }) {
	[editor] = useLexicalComposerContext();
	return (
		<AppshotsPlugin
			draftKey={draftKey}
			workspaceRoot={null}
			imagesSupported
			disabled={false}
		/>
	);
}
const render = (draftKey: string) => (
	<LexicalComposer
		initialConfig={{
			namespace: "appshots",
			nodes: [ImageBadgeNode, FileBadgeNode],
			onError(error) {
				throw error;
			},
		}}
	>
		<Harness draftKey={draftKey} />
	</LexicalComposer>
);
beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	localStorage.clear();
	ipc.mockReset();
	vi.mocked(toast.error).mockClear();
	mockIPC(ipc, { shouldMockEvents: true });
	host = document.createElement("div");
	document.body.append(host);
	root = createRoot(host);
});
afterEach(async () => {
	await act(async () => root.unmount());
	host.remove();
	clearMocks();
	vi.unstubAllGlobals();
});
it("leaves a late capture pending when the person switches tasks", async () => {
	let deliver!: (value: unknown) => void;
	ipc.mockImplementation((cmd, args) => {
		if (cmd === "appshots_status") return { supported: true };
		if (cmd === "appshots_pending")
			return args.draftKey === "a"
				? new Promise((resolve) => {
						deliver = resolve;
					})
				: [];
	});
	await act(async () => root.render(render("a")));
	await act(async () => root.render(render("b")));
	await act(async () =>
		deliver([{ id: "one", draftKey: "a", path: "/appshot.png" }]),
	);
	expect(editor.getEditorState().read($readComposerContext)).toEqual([]);
	expect(ipc.mock.calls.some(([cmd]) => cmd === "appshots_acknowledge")).toBe(
		false,
	);
});
it("saves to the draft before acknowledging and handles repeated ready events", async () => {
	const shots = [{ id: "one", draftKey: "a", path: "/appshot.png" }];
	ipc.mockImplementation((cmd) => {
		if (cmd === "appshots_status") return { supported: true };
		if (cmd === "appshots_pending") return shots;
		if (cmd === "appshots_acknowledge")
			expect(loadDraft("a")).toContain("@/appshot.png");
	});
	await act(async () => root.render(render("a")));
	await act(async () => emit("appshots-ready", "a"));
	expect(editor.getEditorState().read($readComposerContext)).toHaveLength(1);
	expect(
		ipc.mock.calls.filter(([cmd]) => cmd === "appshots_acknowledge"),
	).toHaveLength(2);
});

it("stays quiet with an older backend even after repeated focus and task switches", async () => {
	ipc.mockRejectedValue("Command appshots_status not found");
	await act(async () => root.render(render("a")));
	for (let i = 0; i < 3; i++) {
		await act(async () => window.dispatchEvent(new Event("focus")));
	}
	await act(async () => root.render(render("b")));
	expect(ipc.mock.calls.every(([cmd]) => cmd === "appshots_status")).toBe(true);
	expect(toast.error).not.toHaveBeenCalled();
});

it("does not activate or poll Appshots on unsupported platforms", async () => {
	ipc.mockResolvedValue({ supported: false });
	await act(async () => root.render(render("a")));
	await act(async () => window.dispatchEvent(new Event("focus")));
	expect(ipc).toHaveBeenCalledTimes(1);
	expect(toast.error).not.toHaveBeenCalled();
});

it("keeps passive delivery failures quiet but reports failure after an explicit capture", async () => {
	ipc.mockImplementation((cmd) => {
		if (cmd === "appshots_status") return { supported: true };
		if (cmd === "appshots_pending") return Promise.reject("unavailable");
	});
	await act(async () => root.render(render("a")));
	await act(async () => window.dispatchEvent(new Event("focus")));
	expect(toast.error).not.toHaveBeenCalled();
	await act(async () => emit("appshots-ready", "a"));
	expect(toast.error).toHaveBeenCalledExactlyOnceWith(
		"appshots.errors.unavailable",
		{ id: "appshots-error" },
	);
});
