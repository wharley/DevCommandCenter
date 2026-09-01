import { describe, expect, it, vi } from "vitest";
import {
	BROWSER_OCCLUDER_ATTRIBUTE,
	createBrowserOcclusionCommandQueue,
	isBrowserOccluded,
	isBrowserOccluderVisible,
	rectanglesIntersect,
} from "./browser-occlusion";

describe("browser occlusion geometry", () => {
	it("only treats positive-area overlap as an occlusion", () => {
		const viewport = { left: 100, top: 100, right: 500, bottom: 500 };
		expect(rectanglesIntersect(viewport, { left: 200, top: 200, right: 300, bottom: 300 })).toBe(true);
		expect(rectanglesIntersect(viewport, { left: 500, top: 200, right: 700, bottom: 300 })).toBe(false);
		expect(rectanglesIntersect(viewport, { left: -100, top: -100, right: 50, bottom: 50 })).toBe(false);
	});

	it("handles a portal that contains the viewport", () => {
		expect(
			rectanglesIntersect(
				{ left: 240, top: 180, right: 880, bottom: 760 },
				{ left: 0, top: 0, right: 1200, bottom: 900 },
			),
		).toBe(true);
	});

	it("keeps accessibility-hidden and exit-animation portals conservative", () => {
		const element = document.createElement("div");
		document.body.appendChild(element);
		viRect(element, 10, 10, 100, 100);
		element.setAttribute("aria-hidden", "true");
		element.setAttribute("data-state", "closed");
		expect(isBrowserOccluderVisible(element)).toBe(true);
		element.hidden = true;
		expect(isBrowserOccluderVisible(element)).toBe(false);
		element.hidden = false;
		Object.defineProperty(element, "getBoundingClientRect", { value: () => ({ width: 0, height: 0 }) });
		expect(isBrowserOccluderVisible(element)).toBe(false);
		element.remove();
	});

	it("combines multiple marked surfaces and ignores non-overlapping portals", () => {
		const viewport = document.createElement("div");
		const outside = document.createElement("div");
		const inside = document.createElement("div");
		for (const element of [viewport, outside, inside]) document.body.appendChild(element);
		viRect(viewport, 400, 100, 300, 300);
		viRect(outside, 0, 0, 200, 200);
		viRect(inside, 600, 200, 120, 120);
		outside.setAttribute(BROWSER_OCCLUDER_ATTRIBUTE, "true");
		inside.setAttribute(BROWSER_OCCLUDER_ATTRIBUTE, "true");
		expect(isBrowserOccluded(viewport)).toBe(true);
		inside.remove();
		expect(isBrowserOccluded(viewport)).toBe(false);
		viewport.remove();
		outside.remove();
	});

	it("coalesces pending visibility changes and preserves in-flight order", async () => {
		const queue = createBrowserOcclusionCommandQueue();
		const calls: boolean[] = [];
		let releaseFirst!: () => void;
		const first = new Promise<void>((resolve) => {
			releaseFirst = resolve;
		});
		queue.enqueue(false, async () => {
			calls.push(false);
			await first;
		});
		queue.enqueue(true, async () => {
			calls.push(true);
		});
		releaseFirst();
		await Promise.resolve();
		await Promise.resolve();
		expect(calls).toEqual([false, true]);
		await new Promise<void>((resolve) => setTimeout(resolve, 0));

		let releaseTrue!: () => void;
		const trueCommand = new Promise<void>((resolve) => {
			releaseTrue = resolve;
		});
		queue.enqueue(true, async () => {
			calls.push(true);
			await trueCommand;
		});
		queue.enqueue(false, async () => {
			calls.push(false);
		});
		releaseTrue();
		await new Promise<void>((resolve) => setTimeout(resolve, 0));
		expect(calls).toEqual([false, true, true, false]);
	});
});

function viRect(element: HTMLElement, left: number, top: number, width: number, height: number) {
	Object.defineProperty(element, "getBoundingClientRect", {
		configurable: true,
		value: () => ({ left, top, right: left + width, bottom: top + height, width, height }),
	});
}
