import type { SessionEventRecord } from "@dcc/contracts";
import { beforeEach, describe, expect, it } from "vitest";

import {
	SESSION_TIMELINE_CACHE_LIMIT,
	useSessionTimelineStore,
} from "./session-timeline-store";

function record(sessionId: string, sequence: number): SessionEventRecord {
	return { sessionId, eventId: `${sessionId}-${sequence}`, sequence } as SessionEventRecord;
}

function ready(sessionId: string, history = [record(sessionId, 1)]) {
	useSessionTimelineStore.getState().publish({ sessionId, history, liveEvents: [], ready: true });
}

function resetting(sessionId: string) {
	useSessionTimelineStore
		.getState()
		.publish({ sessionId, history: [], liveEvents: [], ready: false });
}

const entry = (sessionId: string) => useSessionTimelineStore.getState().entries[sessionId];

describe("session timeline store", () => {
	beforeEach(() => useSessionTimelineStore.getState().clear());

	it("keeps the shown conversation while a rehydrate loads", () => {
		const history = [record("a", 1), record("a", 2)];
		ready("a", history);
		resetting("a");

		expect(entry("a")).toMatchObject({ ready: true, refreshing: true });
		expect(entry("a")!.history).toBe(history);
	});

	it("replaces the conversation when the new snapshot lands", () => {
		ready("a");
		resetting("a");
		const fresh = [record("a", 1), record("a", 2), record("a", 3)];
		ready("a", fresh);

		expect(entry("a")).toMatchObject({ ready: true, refreshing: false });
		expect(entry("a")!.history).toBe(fresh);
	});

	it("does not republish while already refreshing", () => {
		ready("a");
		resetting("a");
		const refreshing = entry("a");
		resetting("a");

		expect(entry("a")).toBe(refreshing);
	});

	it("starts empty when nothing was shown yet", () => {
		resetting("a");

		expect(entry("a")).toMatchObject({ ready: false, active: true, history: [] });
	});

	it("drops the cached timeline once the legacy feed takes over", () => {
		ready("a");
		useSessionTimelineStore.getState().fallBackToLegacy("a");
		resetting("a");

		expect(entry("a")).toMatchObject({ ready: false, active: true, history: [] });
	});

	it("evicts the least recently published conversations", () => {
		for (let index = 0; index <= SESSION_TIMELINE_CACHE_LIMIT; index += 1) {
			ready(`s${index}`);
		}
		resetting("s1");
		ready("latest");

		const ids = Object.keys(useSessionTimelineStore.getState().entries);
		expect(ids).toHaveLength(SESSION_TIMELINE_CACHE_LIMIT);
		expect(ids).not.toContain("s0");
		expect(ids).not.toContain("s2");
		expect(ids).toContain("s1");
		expect(ids).toContain("latest");
	});
});
