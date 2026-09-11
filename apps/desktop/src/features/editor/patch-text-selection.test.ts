import { afterEach, describe, expect, it, vi } from "vitest";
import { readPatchTextSelection } from "./patch-text-selection";

afterEach(() => vi.restoreAllMocks());

function fixture() {
	const container = document.createElement("div");
	const host = document.createElement("diffs-container");
	container.append(host);
	const root = host.attachShadow({ mode: "open" });
	root.innerHTML = '<div data-line="13" data-line-type="change-deletion"><span>old</span></div><div data-line="14" data-line-type="change-addition"><span>new</span></div>';
	return { container, root, start: root.querySelector("span")!.firstChild!, end: root.querySelectorAll("span")[1]!.firstChild! };
}

describe("native patch text selection", () => {
	it.each(["single line", "multiple lines"])(
		"reads a WebKit %s selection even when the document reports it as collapsed",
		(mode) => {
			const { container, start, end } = fixture();
			const endNode = mode === "single line" ? start : end;
			vi.spyOn(window, "getSelection").mockReturnValue({
				isCollapsed: true,
				anchorNode: container,
				focusNode: container,
				getComposedRanges: () => [new StaticRange({
					startContainer: start,
					startOffset: 0,
					endContainer: endNode,
					endOffset: 3,
				})],
			} as unknown as Selection);
			expect(readPatchTextSelection(container)).toEqual({
				start: 13,
				end: mode === "single line" ? 13 : 14,
				side: "deletions",
				endSide: mode === "single line" ? "deletions" : "additions",
			});
		},
	);
	it("does not offer an action for a collapsed composed range", () => {
		const { container, start } = fixture();
		vi.spyOn(window, "getSelection").mockReturnValue({
			isCollapsed: true,
			getComposedRanges: () => [new StaticRange({
				startContainer: start,
				startOffset: 1,
				endContainer: start,
				endOffset: 1,
			})],
		} as unknown as Selection);
		expect(readPatchTextSelection(container)).toBeNull();
	});
	it("reads text endpoints in a shadow root, including reversed selection", () => {
		const { container, start, end } = fixture();
		vi.spyOn(window, "getSelection").mockReturnValue({ isCollapsed: false, anchorNode: end, focusNode: start } as unknown as Selection);
		expect(readPatchTextSelection(container)).toEqual({ start: 14, end: 13, side: "additions", endSide: "deletions" });
	});
	it("uses composed ranges when a browser retargets selection to the host", () => {
		const { container, start, end } = fixture();
		vi.spyOn(window, "getSelection").mockReturnValue({ isCollapsed: false, getComposedRanges: () => [{ startContainer: start, endContainer: end }] } as unknown as Selection);
		expect(readPatchTextSelection(container)).toEqual({ start: 13, end: 14, side: "deletions", endSide: "additions" });
	});
	it("ignores a caret and selections outside the current diff", () => {
		const { container, start, end } = fixture();
		const selection = vi.spyOn(window, "getSelection");
		selection.mockReturnValue({ isCollapsed: true, anchorNode: start, focusNode: end } as unknown as Selection);
		expect(readPatchTextSelection(container)).toBeNull();
		selection.mockReturnValue({ isCollapsed: false, anchorNode: start, focusNode: document.createTextNode("another card") } as unknown as Selection);
		expect(readPatchTextSelection(container)).toBeNull();
	});
});
