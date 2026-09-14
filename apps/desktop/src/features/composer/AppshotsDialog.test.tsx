import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { AppshotsDialog } from "./AppshotsDialog";

vi.mock("react-i18next", () => {
	const t = (key: string) => key;
	return { useTranslation: () => ({ t }) };
});
let root: Root;
let host: HTMLDivElement;
let granted = true;
const close = vi.fn();
const ipc = vi.fn();
const windows = Array.from({ length: 8 }, (_, index) => ({
	bundleId: `app.${index}`,
	name: `App ${index}`,
	title: `Window ${index}`,
	pid: index + 1,
	windowId: index + 10,
	x: 0,
	y: 0,
	width: 800,
	height: 600,
}));
const button = (text: string) =>
	[...document.querySelectorAll("button")].find((item) =>
		item.textContent?.includes(text),
	)!;
beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	granted = true;
	close.mockClear();
	ipc.mockReset();
	ipc.mockImplementation((cmd, args) => {
		if (cmd === "appshots_status")
			return {
				supported: true,
				screenRecording: granted,
				shortcut: null,
				shortcutError: false,
				targets: granted && args.includeTargets ? windows : [],
			};
		if (cmd === "appshots_preview")
			return {
				id: `preview-${args.target.windowId}`,
				dataUrl: "data:image/png;base64,cHJldmlldw==",
			};
	});
	mockIPC(ipc);
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
it("previews only one page and attaches the selected snapshots to the original draft", async () => {
	await act(async () =>
		root.render(<AppshotsDialog draftKey="draft-a" onClose={close} />),
	);
	expect(
		ipc.mock.calls.filter(([cmd]) => cmd === "appshots_preview"),
	).toHaveLength(6);
	await act(async () => button("App 1").click());
	await act(async () =>
		document
			.querySelector<HTMLButtonElement>('[aria-label="appshots.next"]')!
			.click(),
	);
	expect(
		ipc.mock.calls.filter(([cmd]) => cmd === "appshots_preview"),
	).toHaveLength(8);
	await act(async () => button("App 7").click());
	await act(async () => button("appshots.attach").click());
	expect(ipc).toHaveBeenCalledWith("appshots_attach", {
		draftKey: "draft-a",
		ids: ["preview-11", "preview-17"],
	});
	expect(close).toHaveBeenCalledOnce();
	expect(
		ipc.mock.calls.some(([cmd]) => cmd === "appshots_request_access"),
	).toBe(false);
});
it("shows permission setup without attempting screenshots", async () => {
	granted = false;
	await act(async () =>
		root.render(<AppshotsDialog draftKey="draft" onClose={close} />),
	);
	expect(document.body.textContent).toContain("appshots.permissionHint");
	expect(button("appshots.attach").disabled).toBe(true);
	expect(ipc.mock.calls.some(([cmd]) => cmd === "appshots_preview")).toBe(
		false,
	);
});

it("explains that an old native backend needs restarting when capture is requested", async () => {
	ipc.mockRejectedValue("Command appshots_status not found");
	await act(async () => root.render(<AppshotsDialog draftKey="draft" onClose={close} />));
	expect(document.body.textContent).toContain("appshots.errors.backendOutdated");
	expect(document.body.textContent).not.toContain("appshots.errors.unsupported");
	expect(button("appshots.attach").disabled).toBe(true);
});
it("allows cancel without saving previews and shows a capture failure per window", async () => {
	const original = ipc.getMockImplementation()!;
	ipc.mockImplementation((cmd, args) =>
		cmd === "appshots_preview"
			? Promise.reject("windowUnavailable")
			: original(cmd, args),
	);
	await act(async () =>
		root.render(<AppshotsDialog draftKey="draft" onClose={close} />),
	);
	expect(document.body.textContent).toContain(
		"appshots.errors.windowUnavailable",
	);
	expect(button("appshots.attach").disabled).toBe(true);
	await act(async () => button("appshots.cancel").click());
	expect(close).toHaveBeenCalledOnce();
	expect(ipc.mock.calls.some(([cmd]) => cmd === "appshots_attach")).toBe(false);
});
