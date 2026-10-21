import { describe, expect, it } from "vitest";
import {
	arrowScrollStep,
	scrollDeltaToReveal,
	tabStripOverflow,
} from "./scrollable-tab-strip.logic";

describe("tabStripOverflow", () => {
	it("reports nothing when every tab fits", () => {
		expect(tabStripOverflow(0, 500, 500)).toEqual({ left: false, right: false });
	});

	it("reports the hidden side at each end of the scroll", () => {
		expect(tabStripOverflow(0, 300, 900)).toEqual({ left: false, right: true });
		expect(tabStripOverflow(300, 300, 900)).toEqual({ left: true, right: true });
		expect(tabStripOverflow(600, 300, 900)).toEqual({ left: true, right: false });
	});

	it("ignores sub-pixel leftovers", () => {
		expect(tabStripOverflow(599.5, 300, 900)).toEqual({ left: true, right: false });
		expect(tabStripOverflow(0.5, 300, 900)).toEqual({ left: false, right: true });
	});
});

describe("scrollDeltaToReveal", () => {
	it("leaves a visible tab alone", () => {
		expect(scrollDeltaToReveal(100, 200, 0, 400, 28)).toBe(0);
	});

	it("scrolls left to clear the left arrow", () => {
		expect(scrollDeltaToReveal(110, 200, 100, 400, 28)).toBe(-18);
	});

	it("scrolls right to clear the right arrow", () => {
		expect(scrollDeltaToReveal(450, 560, 100, 400, 28)).toBe(88);
	});

	it("aligns a tab wider than the strip to its start", () => {
		expect(scrollDeltaToReveal(50, 600, 0, 300, 28)).toBe(22);
	});
});

describe("arrowScrollStep", () => {
	it("moves most of the strip, never less than a tab", () => {
		expect(arrowScrollStep(500)).toBe(350);
		expect(arrowScrollStep(60)).toBe(80);
	});
});
