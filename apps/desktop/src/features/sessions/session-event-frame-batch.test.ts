import type { CoreEvent } from "@dcc/contracts";
import { describe, expect, it, vi } from "vitest";
import { SessionEventFrameBatch } from "./session-event-frame-batch";

const event = (sessionId: string, index: number) =>
	({ sessionTurnDelta: { session_id: sessionId, turn_id: "t", content: String(index) } }) as CoreEvent;

function manualFrames() {
	let nextHandle = 0;
	const callbacks = new Map<number, () => void>();
	return {
		scheduler: {
			schedule(callback: () => void) {
				nextHandle += 1;
				callbacks.set(nextHandle, callback);
				return nextHandle;
			},
			cancel(handle: number) {
				callbacks.delete(handle);
			},
		},
		flushFrame() {
			const scheduled = [...callbacks.values()];
			callbacks.clear();
			for (const callback of scheduled) callback();
		},
		pendingFrames: () => callbacks.size,
	};
}

describe("SessionEventFrameBatch", () => {
	it("publishes a frame once and preserves native event order", () => {
		const frames = manualFrames();
		const flush = vi.fn();
		const batch = new SessionEventFrameBatch(flush, frames.scheduler);
		const events = [event("s", 1), event("s", 2), event("s", 3)];

		for (const next of events) batch.enqueue(next);

		expect(frames.pendingFrames()).toBe(1);
		expect(flush).not.toHaveBeenCalled();
		frames.flushFrame();
		expect(flush).toHaveBeenCalledTimes(1);
		expect(flush).toHaveBeenCalledWith(events);
	});

	it("starts a new batch for the next frame", () => {
		const frames = manualFrames();
		const flush = vi.fn();
		const batch = new SessionEventFrameBatch(flush, frames.scheduler);

		batch.enqueue(event("s", 1));
		frames.flushFrame();
		batch.enqueue(event("s", 2));
		frames.flushFrame();

		expect(flush).toHaveBeenCalledTimes(2);
	});

	it("removes purged sessions before publication without reordering survivors", () => {
		const frames = manualFrames();
		const flush = vi.fn();
		const batch = new SessionEventFrameBatch(flush, frames.scheduler);
		const first = event("keep", 1);
		const removed = event("remove", 2);
		const last = event("keep", 3);

		batch.enqueue(first);
		batch.enqueue(removed);
		batch.enqueue(last);
		batch.purgeSession("remove");
		frames.flushFrame();

		expect(flush).toHaveBeenCalledWith([first, last]);
	});

	it("cancels pending publication on cleanup", () => {
		const frames = manualFrames();
		const flush = vi.fn();
		const batch = new SessionEventFrameBatch(flush, frames.scheduler);

		batch.enqueue(event("s", 1));
		batch.dispose();
		frames.flushFrame();

		expect(frames.pendingFrames()).toBe(0);
		expect(flush).not.toHaveBeenCalled();
	});
});
