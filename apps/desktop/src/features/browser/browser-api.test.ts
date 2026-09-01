import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: invokeMock,
}));

import {
	extractBrowserContext,
	getBrowserControlStatus,
	armBrowserControl,
	disarmBrowserControl,
	navigateBrowser,
	openBrowser,
	setBrowserBounds,
	setBrowserOccluded,
} from "./browser-api";
import {
	readBrowserBounds,
	browserControlExpiryDelay,
	snapBrowserBoundsToDevicePixels,
} from "./workspace-browser-surface";

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
			restoreLastUrl: false,
			bounds: { x: 0, y: 44, width: 900, height: 700 },
		});
	});

	it("serializes durable URL restore only when the caller explicitly opts in", async () => {
		await openBrowser({
			workspaceId: "workspace-1",
			sessionId: null,
			restoreLastUrl: true,
			bounds: { x: 0, y: 44, width: 900, height: 700 },
		});

		expect(invokeMock).toHaveBeenCalledWith("browser_open", expect.objectContaining({
			restoreLastUrl: true,
			initialUrl: null,
		}));
	});

	it("can start hidden when a marked overlay already covers the viewport", async () => {
		await openBrowser({
			workspaceId: "workspace-1",
			sessionId: null,
			bounds: { x: 0, y: 0, width: 800, height: 600 },
			initialOccluded: true,
		});

		expect(invokeMock).toHaveBeenCalledWith("browser_open", expect.objectContaining({
			initialOccluded: true,
		}));
	});

	it("does not expose arbitrary native webview operations", async () => {
		await navigateBrowser({ workspaceId: "workspace-1", sessionId: null, lifecycleToken: 1, url: "https://example.com" });
		await setBrowserBounds({
			workspaceId: "workspace-1",
			sessionId: null,
			lifecycleToken: 1,
			bounds: { x: 0, y: 0, width: 1, height: 1 },
		});

		expect(invokeMock).toHaveBeenNthCalledWith(2, "browser_set_bounds", {
			workspaceId: "workspace-1",
			sessionId: null,
			lifecycleToken: 1,
			bounds: { x: 0, y: 0, width: 1, height: 1 },
		});
		expect(invokeMock).not.toHaveBeenCalledWith("webview_create", expect.anything());
	});

	it("keeps temporary occlusion scoped to the current browser lifecycle", async () => {
		await setBrowserOccluded({
			workspaceId: "workspace-1",
			sessionId: "session-1",
			lifecycleToken: 12,
			occluded: true,
		});

		expect(invokeMock).toHaveBeenCalledWith("browser_set_occluded", {
			workspaceId: "workspace-1",
			sessionId: "session-1",
			lifecycleToken: 12,
			occluded: true,
		});
	});

	it("requests page context only through the scoped backend command", async () => {
		await extractBrowserContext({
			workspaceId: "workspace-1",
			sessionId: "session-1",
			lifecycleToken: 1,
		});

		expect(invokeMock).toHaveBeenCalledWith("browser_extract_context", {
			workspaceId: "workspace-1",
			sessionId: "session-1",
			lifecycleToken: 1,
		});
		expect(invokeMock).not.toHaveBeenCalledWith("webview_eval", expect.anything());
	});

	it("keeps control consent lifecycle-scoped and exposes no token", async () => {
		const input = { workspaceId: "workspace-1", sessionId: "session-1", lifecycleToken: 9 };
		await getBrowserControlStatus(input);
		await armBrowserControl(input);
		await disarmBrowserControl(input);

		expect(invokeMock).toHaveBeenNthCalledWith(1, "browser_control_status", input);
		expect(invokeMock).toHaveBeenNthCalledWith(2, "browser_arm_control", input);
		expect(invokeMock).toHaveBeenNthCalledWith(3, "browser_disarm_control", input);
		expect(invokeMock.mock.calls.flat()).not.toContain("token");
	});

	it("uses one bounded expiry delay rather than an interval", () => {
		expect(browserControlExpiryDelay(60_000.2)).toBe(60_001);
		expect(browserControlExpiryDelay(-1)).toBe(0);
		expect(browserControlExpiryDelay(Number.NaN)).toBe(0);
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

	it("snaps fractional edges outward to device pixels", () => {
		expect(
			snapBrowserBoundsToDevicePixels(
				{ x: 10.25, y: 20.25, width: 100.1, height: 200.1 },
				2,
			),
		).toEqual({ x: 10, y: 20, width: 100.5, height: 200.5 });
	});
});
