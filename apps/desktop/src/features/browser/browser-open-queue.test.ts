import { describe, expect, it } from "vitest";
import type { BrowserOpenRequest } from "./browser-api";
import { browserOpenForTask, enqueueBrowserOpen, hasBrowserOpen, removeBrowserOpen } from "./browser-open-queue";

const request = (requestId: string, workspaceId: string, sessionId: string, url: string): BrowserOpenRequest => ({
	requestId, workspaceId, sessionId, url, providerId: "claude", reason: "", expiresAtMs: 0,
});

describe("browser open queue", () => {
	it("keeps one approved open per task when two tasks are approved", () => {
		const a = request("a", "ws-a", "s-a", "http://localhost:3000/");
		const b = request("b", "ws-b", "s-b", "http://localhost:5173/");
		const queue = enqueueBrowserOpen(enqueueBrowserOpen({}, a), b);
		expect(browserOpenForTask(queue, "ws-a", "s-a")).toBe(a);
		expect(browserOpenForTask(queue, "ws-b", "s-b")).toBe(b);
	});

	it("never hands one task the request of another", () => {
		const queue = enqueueBrowserOpen({}, request("b", "ws-b", "s-b", "http://localhost:5173/"));
		expect(browserOpenForTask(queue, "ws-a", "s-a")).toBeNull();
		expect(browserOpenForTask(queue, "ws-b", "s-a")).toBeNull();
		expect(browserOpenForTask(queue, "ws-b", null)).toBeNull();
	});

	it("replaces an older approval of the same task", () => {
		const first = request("1", "ws", "s", "http://localhost:3000/");
		const second = request("2", "ws", "s", "http://localhost:3001/");
		const queue = enqueueBrowserOpen(enqueueBrowserOpen({}, first), second);
		expect(browserOpenForTask(queue, "ws", "s")).toBe(second);
		expect(hasBrowserOpen(queue, "1")).toBe(false);
	});

	it("removes only the opened request", () => {
		const a = request("a", "ws-a", "s-a", "http://localhost:3000/");
		const b = request("b", "ws-b", "s-b", "http://localhost:5173/");
		const queue = removeBrowserOpen(enqueueBrowserOpen(enqueueBrowserOpen({}, a), b), "a");
		expect(hasBrowserOpen(queue, "a")).toBe(false);
		expect(browserOpenForTask(queue, "ws-b", "s-b")).toBe(b);
		expect(removeBrowserOpen(queue, "missing")).toBe(queue);
	});
});
