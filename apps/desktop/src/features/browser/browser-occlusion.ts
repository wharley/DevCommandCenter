import { useEffect, useRef, type RefObject } from "react";
import { setBrowserOccluded, type BrowserBounds } from "./browser-api";

/** Stable opt-in marker for DCC surfaces that can cover a native browser. */
export const BROWSER_OCCLUDER_ATTRIBUTE = "data-dcc-browser-occluder";
const BROWSER_OCCLUDER_SELECTOR = `[${BROWSER_OCCLUDER_ATTRIBUTE}]`;

export type BrowserViewportRect = Pick<DOMRect, "left" | "top" | "right" | "bottom">;

export function rectanglesIntersect(a: BrowserViewportRect, b: BrowserViewportRect) {
	return b.right > a.left && b.left < a.right && b.bottom > a.top && b.top < a.bottom;
}

export function isBrowserOccluderVisible(element: HTMLElement) {
	if (!element.isConnected || element.hidden) {
		return false;
	}
	const style = window.getComputedStyle(element);
	if (style.display === "none" || style.visibility === "hidden") return false;
	const rect = element.getBoundingClientRect();
	return rect.width > 0 && rect.height > 0;
}

export function isBrowserOccluded(viewport: HTMLElement) {
	const viewportRect = viewport.getBoundingClientRect();
	return Array.from(document.querySelectorAll<HTMLElement>(BROWSER_OCCLUDER_SELECTOR)).some(
		(element) => isBrowserOccluderVisible(element) && rectanglesIntersect(viewportRect, element.getBoundingClientRect()),
	);
}

function readViewportBounds(viewport: HTMLElement): BrowserBounds {
	const rect = viewport.getBoundingClientRect();
	const scale = Number.isFinite(window.devicePixelRatio) && window.devicePixelRatio > 0
		? window.devicePixelRatio
		: 1;
	const left = Math.floor(Math.max(0, rect.left) * scale);
	const top = Math.floor(Math.max(0, rect.top) * scale);
	const right = Math.ceil((Math.max(0, rect.left) + Math.max(1, rect.width)) * scale);
	const bottom = Math.ceil((Math.max(0, rect.top) + Math.max(1, rect.height)) * scale);
	return {
		x: left / scale,
		y: top / scale,
		width: Math.max(1, (right - left) / scale),
		height: Math.max(1, (bottom - top) / scale),
	};
}

/**
 * Watches opt-in DCC portals and reports whether any visible portal intersects
 * the browser viewport. All sources are event-driven; layout work is coalesced
 * into one animation frame and no idle polling is used.
 */
export function observeBrowserOcclusion(
	viewport: HTMLElement,
	onChange: (occluded: boolean) => void,
): () => void {
	let frame: number | null = null;
	let disposed = false;
	let lastValue: boolean | null = null;
	const observed = new Set<HTMLElement>();
	const resizeObserver = typeof ResizeObserver === "undefined"
		? null
		: new ResizeObserver(schedule);

	const measure = () => {
		frame = null;
		if (disposed) return;
		const viewportRect = viewport.getBoundingClientRect();
		const occluders = Array.from(
			document.querySelectorAll<HTMLElement>(BROWSER_OCCLUDER_SELECTOR),
		);
		for (const element of observed) {
			if (!element.isConnected) {
				resizeObserver?.unobserve(element);
				observed.delete(element);
			}
		}
		for (const element of occluders) {
			if (!observed.has(element)) {
				observed.add(element);
				resizeObserver?.observe(element);
			}
		}
		const nextValue = occluders.some(
			(element) => isBrowserOccluderVisible(element) && rectanglesIntersect(viewportRect, element.getBoundingClientRect()),
		);
		if (nextValue !== lastValue) {
			lastValue = nextValue;
			onChange(nextValue);
		}
	};

	function schedule() {
		if (disposed || frame !== null) return;
		frame = requestAnimationFrame(measure);
	}

	const mutationObserver = typeof MutationObserver === "undefined"
		? null
		: new MutationObserver(schedule);
	mutationObserver?.observe(document.body, {
		childList: true,
		subtree: true,
		attributes: true,
		attributeFilter: ["class", "style", "hidden", "aria-hidden", "data-state", BROWSER_OCCLUDER_ATTRIBUTE],
	});
	window.addEventListener("resize", schedule);
	window.addEventListener("scroll", schedule, true);
	window.visualViewport?.addEventListener("resize", schedule);
	window.visualViewport?.addEventListener("scroll", schedule);
	resizeObserver?.observe(viewport);
	schedule();

	return () => {
		disposed = true;
		if (frame !== null) cancelAnimationFrame(frame);
		mutationObserver?.disconnect();
		resizeObserver?.disconnect();
		window.removeEventListener("resize", schedule);
		window.removeEventListener("scroll", schedule, true);
		window.visualViewport?.removeEventListener("resize", schedule);
		window.visualViewport?.removeEventListener("scroll", schedule);
	};
}

export type BrowserOcclusionCommandQueue = {
	enqueue: (occluded: boolean, command: () => Promise<void>) => void;
	clearPending: () => void;
};

/**
 * A single coalescing queue per mounted Browser lifecycle. A pending command is
 * replaced by the newest desired state, while an in-flight command completes
 * in order; this prevents an old false intent from being sent after a newer
 * true intent when React rerenders the force-occlusion prop.
 */
export function createBrowserOcclusionCommandQueue(): BrowserOcclusionCommandQueue {
	let pending: { occluded: boolean; command: () => Promise<void> } | null = null;
	let running = false;
	const drain = async () => {
		while (pending) {
			const next = pending;
			pending = null;
			try {
				await next.command();
			} catch {
				// A stale lifecycle or closed Browser is expected during unmount.
			}
		}
		running = false;
	};
	return {
		enqueue(occluded, command) {
			pending = { occluded, command };
			if (!running) {
				running = true;
				void drain();
			}
		},
		clearPending() {
			pending = null;
		},
	};
}

/** Connects the observer to the scoped native WebView visibility command. */
export function useBrowserOcclusion({
	viewportRef,
	workspaceId,
	sessionId,
	lifecycleToken,
	forceOccluded = false,
}: {
	viewportRef: RefObject<HTMLElement | null>;
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number | null;
	forceOccluded?: boolean;
}) {
	const commandQueueRef = useRef<BrowserOcclusionCommandQueue | null>(null);
	if (commandQueueRef.current === null) {
		commandQueueRef.current = createBrowserOcclusionCommandQueue();
	}
	useEffect(() => {
		const viewport = viewportRef.current;
		if (!viewport || lifecycleToken === null) return;
		commandQueueRef.current?.clearPending();
		let disposed = false;
		let measuredOccluded = false;
		let lastRequested: boolean | null = null;
		const publish = (nextMeasured: boolean) => {
			measuredOccluded = nextMeasured;
			const next = measuredOccluded || forceOccluded;
			if (next === lastRequested) return;
			lastRequested = next;
			commandQueueRef.current?.enqueue(next, async () => {
				if (disposed) return;
				await setBrowserOccluded({
					workspaceId,
					sessionId,
					lifecycleToken,
					occluded: next,
					bounds: readViewportBounds(viewport),
				});
			});
		};
		const dispose = observeBrowserOcclusion(viewport, publish);
		if (forceOccluded) publish(true);
		return () => {
			disposed = true;
			dispose();
		};
	}, [forceOccluded, lifecycleToken, sessionId, viewportRef, workspaceId]);
}
