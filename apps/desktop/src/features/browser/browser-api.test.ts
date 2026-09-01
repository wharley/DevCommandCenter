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
	readBrowserAudit,
	navigateBrowser,
	openBrowser,
	setBrowserBounds,
	setBrowserOccluded,
} from "./browser-api";
import {
	readBrowserBounds,
	browserControlExpiryDelay,
	browserAuditTime,
	isCurrentBrowserAuditRequest,
	newestFirstBrowserAuditRecords,
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

	it("reads the trusted audit only through a bounded lifecycle-scoped command", async () => {
		await readBrowserAudit({
			workspaceId: "workspace-1",
			sessionId: "session-1",
			lifecycleToken: 9,
			limit: 50,
		});

		expect(invokeMock).toHaveBeenCalledWith("browser_read_audit", {
			workspaceId: "workspace-1",
			sessionId: "session-1",
			lifecycleToken: 9,
			limit: 50,
		});
	});

	it("uses one bounded expiry delay rather than an interval", () => {
		expect(browserControlExpiryDelay(60_000.2)).toBe(60_001);
		expect(browserControlExpiryDelay(-1)).toBe(0);
		expect(browserControlExpiryDelay(Number.NaN)).toBe(0);
	});

	it("discards audit responses after close, lifecycle, scope, or request changes", () => {
		const scope = { workspaceId: "workspace-1", sessionId: "session-1", lifecycleToken: 3 };
		expect(isCurrentBrowserAuditRequest({
			requestId: 7,
			currentRequestId: 7,
			open: true,
			expected: scope,
			current: scope,
		})).toBe(true);
		expect(isCurrentBrowserAuditRequest({
			requestId: 7,
			currentRequestId: 8,
			open: true,
			expected: scope,
			current: scope,
		})).toBe(false);
		expect(isCurrentBrowserAuditRequest({
			requestId: 7,
			currentRequestId: 7,
			open: false,
			expected: scope,
			current: scope,
		})).toBe(false);
		expect(isCurrentBrowserAuditRequest({
			requestId: 7,
			currentRequestId: 7,
			open: true,
			expected: scope,
			current: { ...scope, lifecycleToken: 4 },
		})).toBe(false);
	});

	it("formats only valid local audit timestamps", () => {
		expect(browserAuditTime(1_700_000_000_000)?.getTime()).toBe(1_700_000_000_000);
		expect(browserAuditTime(-1)).toBeNull();
		expect(browserAuditTime(Number.NaN)).toBeNull();
	});

	it("orders the trusted audit newest-first without mutating the response", () => {
		const records = [
			{ origin: "ui" as const, providerId: null, tool: "dcc_browser_reload" as const, grantState: "notApplicable" as const, outcome: "executed" as const, timestampMs: 3 },
			{ origin: "mcp" as const, providerId: "provider", tool: "dcc_browser_click" as const, grantState: "armed" as const, outcome: "stale" as const, timestampMs: 9 },
		];
		const ordered = newestFirstBrowserAuditRecords(records);
		expect(ordered.map((record) => record.timestampMs)).toEqual([9, 3]);
		expect(records.map((record) => record.timestampMs)).toEqual([3, 9]);
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
