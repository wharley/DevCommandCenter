import { describe, expect, it } from "vitest";
import { isRecapBubbleVisible, isRecapUnread, nextRecapState, recapBoundary } from "./daily-recap";

// Local times, so the 4am boundary is the person's own.
const at = (day: number, hour: number, minute = 0) => new Date(2026, 9, day, hour, minute);

describe("recapBoundary", () => {
	it("is today's 4am after 4am and yesterday's before it", () => {
		expect(recapBoundary(at(6, 9))).toEqual(at(6, 4));
		expect(recapBoundary(at(6, 4))).toEqual(at(6, 4));
		expect(recapBoundary(at(6, 3, 59))).toEqual(at(5, 4));
	});
});

describe("nextRecapState", () => {
	it("starts the first recap a day before the boundary", () => {
		const state = nextRecapState({ current: null, seenTo: null }, at(6, 9));
		expect(state.current).toEqual({
			from: at(5, 4).toISOString(),
			to: at(6, 9).toISOString(),
		});
		expect(isRecapUnread(state)).toBe(true);
	});

	it("keeps the window for the rest of the day", () => {
		const morning = nextRecapState({ current: null, seenTo: null }, at(6, 9));
		expect(nextRecapState(morning, at(6, 14))).toBe(morning);
		expect(nextRecapState(morning, at(7, 3, 59))).toBe(morning);
	});

	it("covers the weekend from where the last recap ended", () => {
		const friday = { current: { from: at(1, 9).toISOString(), to: at(2, 9).toISOString() }, seenTo: at(2, 9).toISOString() };
		const monday = nextRecapState(friday, at(5, 8, 30));
		expect(monday.current).toEqual({
			from: at(2, 9).toISOString(),
			to: at(5, 8, 30).toISOString(),
		});
		expect(isRecapUnread(monday)).toBe(true);
	});

	it("opens the next window at 4am when the app stays open overnight", () => {
		const evening = { current: { from: at(5, 9).toISOString(), to: at(6, 9).toISOString() }, seenTo: null };
		const next = nextRecapState(evening, at(7, 4, 5));
		expect(next.current?.from).toBe(at(6, 9).toISOString());
	});

	it("never reaches back more than a week", () => {
		const old = { current: { from: at(1, 9).toISOString(), to: at(1, 10).toISOString() }, seenTo: null };
		const next = nextRecapState(old, at(20, 9));
		expect(next.current?.from).toBe(new Date(at(20, 9).getTime() - 7 * 24 * 60 * 60 * 1000).toISOString());
	});
});

describe("isRecapUnread", () => {
	it("is read once the person saw the current window", () => {
		const window = { from: at(5, 4).toISOString(), to: at(6, 9).toISOString() };
		expect(isRecapUnread({ current: window, seenTo: window.to })).toBe(false);
		expect(isRecapUnread({ current: window, seenTo: at(5, 9).toISOString() })).toBe(true);
		expect(isRecapUnread({ current: null, seenTo: null })).toBe(false);
	});
});

describe("isRecapBubbleVisible", () => {
	it("shows once per recap, until it is read or closed", () => {
		const window = { from: at(5, 4).toISOString(), to: at(6, 9).toISOString() };
		expect(isRecapBubbleVisible({ current: window, seenTo: null })).toBe(true);
		expect(isRecapBubbleVisible({ current: window, seenTo: null, bubbleDismissedTo: window.to })).toBe(false);
		expect(isRecapBubbleVisible({ current: window, seenTo: window.to })).toBe(false);
		// Closing yesterday's bubble does not silence today's.
		expect(
			isRecapBubbleVisible({ current: window, seenTo: null, bubbleDismissedTo: at(5, 9).toISOString() }),
		).toBe(true);
	});
});
