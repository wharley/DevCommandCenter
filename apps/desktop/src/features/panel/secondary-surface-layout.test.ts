import { afterEach, describe, expect, it, vi } from "vitest";
import {
	canDockSecondarySurface,
	clampSecondarySurfaceWidth,
	clampSecondarySurfaceWidthForContainer,
	DEFAULT_SECONDARY_SURFACE_WIDTH,
	MAX_SECONDARY_SURFACE_WIDTH,
	MIN_SECONDARY_SURFACE_WIDTH,
	persistSecondarySurfaceWidth,
	persistRestorableSecondarySurfaceSelection,
	readRestorableSecondarySurfaceSelection,
	readSecondarySurfaceWidth,
	resolveSecondarySurfaceRestoration,
	secondarySurfaceSelectionStorageKey,
	secondarySurfaceStorageKey,
	shouldRenderGitDiffSurface,
} from "./secondary-surface-layout";

describe("secondary surface layout", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("keeps the default width inside the supported desktop range", () => {
		expect(clampSecondarySurfaceWidth(DEFAULT_SECONDARY_SURFACE_WIDTH)).toBe(
			DEFAULT_SECONDARY_SURFACE_WIDTH,
		);
	});

	it("clamps drag widths to readable bounds", () => {
		expect(clampSecondarySurfaceWidth(MIN_SECONDARY_SURFACE_WIDTH - 80)).toBe(
			MIN_SECONDARY_SURFACE_WIDTH,
		);
		expect(clampSecondarySurfaceWidth(MAX_SECONDARY_SURFACE_WIDTH + 80)).toBe(
			MAX_SECONDARY_SURFACE_WIDTH,
		);
	});

	it("keeps a readable conversation width inside a docked container", () => {
		expect(clampSecondarySurfaceWidthForContainer(840, 900)).toBe(500);
		expect(canDockSecondarySurface(760)).toBe(true);
		expect(canDockSecondarySurface(759)).toBe(false);
	});

	it("routes changed-file review into the visible Inspector", () => {
		expect(shouldRenderGitDiffSurface(false)).toBe(false);
		expect(shouldRenderGitDiffSurface(true)).toBe(true);
		expect(shouldRenderGitDiffSurface()).toBe(true);
	});

	it("persists an independent validated preference per workspace", () => {
		const store = new Map<string, string>();
		vi.stubGlobal("window", {
			localStorage: {
				getItem: (key: string) => store.get(key) ?? null,
				setItem: (key: string, value: string) => store.set(key, value),
				removeItem: (key: string) => store.delete(key),
			},
		});

		persistSecondarySurfaceWidth("workspace/a", 9999);
		expect(readSecondarySurfaceWidth("workspace/a")).toBe(MAX_SECONDARY_SURFACE_WIDTH);
		expect(readSecondarySurfaceWidth("workspace/b")).toBe(
			DEFAULT_SECONDARY_SURFACE_WIDTH,
		);
		expect(secondarySurfaceStorageKey("workspace/a")).not.toBe(
			secondarySurfaceStorageKey("workspace/b"),
		);
	});

	it("restores only safe secondary selections per workspace", () => {
		const store = new Map<string, string>();
		vi.stubGlobal("window", {
			localStorage: {
				getItem: (key: string) => store.get(key) ?? null,
				setItem: (key: string, value: string) => store.set(key, value),
				removeItem: (key: string) => store.delete(key),
			},
		});

		persistRestorableSecondarySurfaceSelection("workspace/a", "plan");
		store.set(secondarySurfaceSelectionStorageKey("workspace/b"), "file-edit");
		expect(readRestorableSecondarySurfaceSelection("workspace/a")).toBe("plan");
		expect(readRestorableSecondarySurfaceSelection("workspace/b")).toBeNull();

		persistRestorableSecondarySurfaceSelection("workspace/a", null);
		expect(readRestorableSecondarySurfaceSelection("workspace/a")).toBeNull();
	});

	it("waits for A's open plan to clear before restoring B's saved plan", () => {
		const duringWorkspaceSwap = resolveSecondarySurfaceRestoration({
			workspaceId: "workspace/b",
			restoredWorkspaceId: "workspace/a",
			surfaceWorkspaceId: "workspace/a",
			hasSurfaceSelection: true,
			storedSelection: "plan",
		});
		expect(duringWorkspaceSwap).toBe("wait");

		const afterPreviousSurfaceClears = resolveSecondarySurfaceRestoration({
			workspaceId: "workspace/b",
			restoredWorkspaceId: "workspace/a",
			surfaceWorkspaceId: null,
			hasSurfaceSelection: false,
			storedSelection: "plan",
		});
		expect(afterPreviousSurfaceClears).toBe("plan");
	});
});
