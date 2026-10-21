/** Width of the faded edge the arrow sits on; a revealed tab stays clear of it. */
export const TAB_STRIP_EDGE_INSET = 40;

export type TabStripOverflow = { left: boolean; right: boolean };

/** Which sides still have tabs out of view. Sub-pixel rounding is ignored. */
export function tabStripOverflow(
	scrollLeft: number,
	clientWidth: number,
	scrollWidth: number,
): TabStripOverflow {
	return {
		left: scrollLeft > 1,
		right: scrollLeft + clientWidth < scrollWidth - 1,
	};
}

/**
 * How far to scroll so the tab sits fully inside the strip, clear of the edge
 * arrows. Zero when it is already visible.
 */
export function scrollDeltaToReveal(
	tabStart: number,
	tabEnd: number,
	viewStart: number,
	viewWidth: number,
	inset: number = TAB_STRIP_EDGE_INSET,
): number {
	const visibleStart = viewStart + inset;
	const visibleEnd = viewStart + viewWidth - inset;
	if (tabEnd - tabStart > visibleEnd - visibleStart || tabStart < visibleStart) {
		return tabStart - visibleStart;
	}
	if (tabEnd > visibleEnd) {
		return tabEnd - visibleEnd;
	}
	return 0;
}

/** One arrow click moves most of a strip, keeping a tab of context. */
export function arrowScrollStep(clientWidth: number): number {
	return Math.max(80, Math.round(clientWidth * 0.7));
}
