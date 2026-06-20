import { describe, expect, it } from "vitest";
import {
	hasDirtyFileSurfaceState,
	resolveFileSurfaceContentState,
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
});
