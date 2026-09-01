import { beforeEach, describe, expect, it } from "vitest";
import {
	BROWSER_SPLIT_BREAKPOINT,
	DEFAULT_BROWSER_SURFACE_WIDTH,
	MIN_BROWSER_SURFACE_WIDTH,
	MAX_BROWSER_SURFACE_WIDTH,
	browserSurfaceStorageKey,
	clampBrowserSurfaceWidth,
	clampBrowserSurfaceWidthForContainer,
	effectiveInspectorCollapsedForBrowserOpen,
	persistBrowserSurfaceWidth,
	readBrowserSurfaceWidth,
	shouldRestoreInspectorAfterBrowserClose,
	shouldSplitBrowser,
} from "./browser-layout";

describe("browser-layout", () => {
	beforeEach(() => window.localStorage.clear());

	it("switches between split and takeover at the usable-column breakpoint", () => {
		expect(shouldSplitBrowser(BROWSER_SPLIT_BREAKPOINT - 1)).toBe(false);
		expect(shouldSplitBrowser(BROWSER_SPLIT_BREAKPOINT)).toBe(true);
		expect(shouldSplitBrowser(Number.NaN)).toBe(false);
	});

	it("clamps persisted and container-constrained widths", () => {
		expect(clampBrowserSurfaceWidth(1)).toBe(MIN_BROWSER_SURFACE_WIDTH);
		expect(clampBrowserSurfaceWidth(9999)).toBe(MAX_BROWSER_SURFACE_WIDTH);
		expect(clampBrowserSurfaceWidth(Number.NaN)).toBe(DEFAULT_BROWSER_SURFACE_WIDTH);
		expect(clampBrowserSurfaceWidthForContainer(800, 900)).toBe(374);
		expect(clampBrowserSurfaceWidthForContainer(9999, 2000)).toBe(MAX_BROWSER_SURFACE_WIDTH);
	});

	it("round-trips a width per workspace without leaking keys", () => {
		persistBrowserSurfaceWidth("workspace/one", 611.4);
		expect(window.localStorage.getItem(browserSurfaceStorageKey("workspace/one"))).toBe("611");
		expect(readBrowserSurfaceWidth("workspace/one")).toBe(611);
		expect(readBrowserSurfaceWidth("workspace/two")).toBe(DEFAULT_BROWSER_SURFACE_WIDTH);
	});

	it("restores Inspector only for the same scope/cycle and untouched state", () => {
		const opened = {
			openedWorkspaceId: "workspace-1",
			openedSessionId: "session-1",
			openedCycle: 4,
			currentCycle: 4,
			openedInspectorCollapsed: false,
		};
		expect(shouldRestoreInspectorAfterBrowserClose({
			...opened,
			currentWorkspaceId: "workspace-1",
			currentSessionId: "session-1",
			currentInspectorCollapsed: true,
		})).toBe(true);
		expect(shouldRestoreInspectorAfterBrowserClose({
			...opened,
			currentWorkspaceId: "workspace-1",
			currentSessionId: "session-1",
			currentInspectorCollapsed: false,
		})).toBe(false);
		expect(shouldRestoreInspectorAfterBrowserClose({
			...opened,
			currentWorkspaceId: "workspace-2",
			currentSessionId: "session-1",
			currentInspectorCollapsed: true,
		})).toBe(false);
	});

	it("retains the pre-terminal Inspector state when Browser opens from an expanded terminal", () => {
		expect(effectiveInspectorCollapsedForBrowserOpen({
			inspectorCollapsed: true,
			inspectorBeforeTerminalExpand: false,
		})).toBe(false);
		expect(effectiveInspectorCollapsedForBrowserOpen({
			inspectorCollapsed: true,
			inspectorBeforeTerminalExpand: null,
		})).toBe(true);
	});
});
