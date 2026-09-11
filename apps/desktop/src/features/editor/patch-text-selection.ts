import type { SelectedLineRange } from "@pierre/diffs";

function lineAt(node: Node | null) {
	const element = node instanceof Element ? node : node?.parentElement;
	const line = element?.closest<HTMLElement>("[data-line]");
	if (!line) return null;
	const number = Number(line.dataset.line);
	if (!Number.isInteger(number) || number < 1) return null;
	return {
		element: line,
		number,
		side: line.dataset.lineType === "change-deletion" ? "deletions" as const : "additions" as const,
	};
}

/** Native text selection lives inside Pierre's shadow root in the desktop webview. */
export function readPatchTextSelection(container: HTMLElement): SelectedLineRange | null {
	const documentSelection = window.getSelection();
	for (const host of container.querySelectorAll("diffs-container")) {
		const root = host.shadowRoot as (ShadowRoot & { getSelection?: () => Selection | null }) | null;
		if (!root) continue;
		const selection = root.getSelection?.() ?? documentSelection;
		if (!selection) continue;
		const composed = (selection as Selection & {
			getComposedRanges?: (options: { shadowRoots: ShadowRoot[] }) => StaticRange[];
		}).getComposedRanges?.({ shadowRoots: [root] })?.[0];
		// WebKit can report Selection.isCollapsed=true for highlighted text in a
		// shadow tree. Inspect the actual composed endpoints before that fallback.
		const collapsed = composed
			? composed.startContainer === composed.endContainer &&
				composed.startOffset === composed.endOffset
			: selection.isCollapsed;
		if (collapsed) continue;
		const start = lineAt(composed?.startContainer ?? selection.anchorNode);
		const end = lineAt(composed?.endContainer ?? selection.focusNode);
		if (!start || !end || !root.contains(start.element) || !root.contains(end.element)) continue;
		return { start: start.number, end: end.number, side: start.side, endSide: end.side };
	}
	return null;
}
