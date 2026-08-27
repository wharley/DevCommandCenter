import { describe, expect, it } from "vitest";
import {
	hasDirtyFileSurfaceState,
	resolveConfirmedFileSurfaceCloseHandler,
	resolveFileSurfaceContentState,
	shouldKeepFileSurfaceMounted,
} from "./file-surface.logic";

describe("file surface logic", () => {
	it("renders the editor once loading completes, even for empty files", () => {
		expect(
			resolveFileSurfaceContentState({
				isError: false,
				isPending: false,
			}),
		).toBe("editor");
	});

	it("prioritizes error and loading states before the editor", () => {
		expect(
			resolveFileSurfaceContentState({
				isError: true,
				isPending: false,
			}),
		).toBe("error");
		expect(
			resolveFileSurfaceContentState({
				isError: false,
				isPending: true,
			}),
		).toBe("loading");
	});

	it("detects dirty tabs before closing all file surfaces", () => {
		expect(
			hasDirtyFileSurfaceState({
				"a.ts": { dirty: false, saving: false },
				"b.ts": { dirty: true, saving: false },
			}),
		).toBe(true);
		expect(
			hasDirtyFileSurfaceState({
				"a.ts": { dirty: false, saving: false },
			}),
		).toBe(false);
	});

	it("uses the editor-confirmed close completion instead of re-requesting a transition", () => {
		let directCloseCalls = 0;
		let transitionRequests = 0;
		const onClose = resolveConfirmedFileSurfaceCloseHandler({
			onFileSurfaceClosed: () => {
				directCloseCalls += 1;
			},
			onCloseSurface: () => {
				transitionRequests += 1;
			},
		});

		onClose();
		expect(directCloseCalls).toBe(1);
		expect(transitionRequests).toBe(0);
	});

	it("unmounts only clean inactive file surfaces", () => {
		expect(
			shouldKeepFileSurfaceMounted("active.ts", "active.ts", {
				dirty: false,
				saving: false,
			}),
		).toBe(true);
		expect(
			shouldKeepFileSurfaceMounted("dirty.ts", "active.ts", {
				dirty: true,
				saving: false,
			}),
		).toBe(true);
		expect(
			shouldKeepFileSurfaceMounted("saving.ts", "active.ts", {
				dirty: false,
				saving: true,
			}),
		).toBe(true);
		expect(
			shouldKeepFileSurfaceMounted("clean.ts", "active.ts", {
				dirty: false,
				saving: false,
			}),
		).toBe(false);
	});
});
