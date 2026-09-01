import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: invokeMock,
}));

import { navigateBrowser, openBrowser, setBrowserBounds } from "./browser-api";
import { readBrowserBounds } from "./workspace-browser-surface";

describe("browser-api", () => {
	beforeEach(() => {
		invokeMock.mockReset();
		invokeMock.mockResolvedValue(undefined);
	});

	it("keeps browser commands narrow and sends the workspace/session scope", async () => {
		invokeMock.mockResolvedValueOnce({
			workspaceId: "workspace-1",
			sessionId: "session-1",
			visible: true,
			url: "http://localhost:3000",
			title: null,
		});

		await openBrowser({
			workspaceId: "workspace-1",
			sessionId: "session-1",
			bounds: { x: 0, y: 44, width: 900, height: 700 },
		});

		expect(invokeMock).toHaveBeenCalledWith("browser_open", {
		workspaceId: "workspace-1",
		sessionId: "session-1",
		initialUrl: null,
		bounds: { x: 0, y: 44, width: 900, height: 700 },
		});
	});

	it("does not expose arbitrary native webview operations", async () => {
		await navigateBrowser({ workspaceId: "workspace-1", sessionId: null, url: "https://example.com" });
		await setBrowserBounds({
			workspaceId: "workspace-1",
			sessionId: null,
			bounds: { x: 0, y: 0, width: 1, height: 1 },
		});

		expect(invokeMock).toHaveBeenNthCalledWith(2, "browser_set_bounds", {
			workspaceId: "workspace-1",
			sessionId: null,
			bounds: { x: 0, y: 0, width: 1, height: 1 },
		});
		expect(invokeMock).not.toHaveBeenCalledWith("webview_create", expect.anything());
	});

	it("normalizes DOM bounds before sending them to native layout", () => {
		const element = document.createElement("div");
		vi.spyOn(element, "getBoundingClientRect").mockReturnValue({
			left: -12,
			top: -4,
			width: 720,
			height: 480,
		} as DOMRect);

		expect(readBrowserBounds(element)).toEqual({ x: 0, y: 0, width: 720, height: 480 });
	});
});
