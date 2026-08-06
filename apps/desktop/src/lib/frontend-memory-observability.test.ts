import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { frontendMemorySnapshot } from "./frontend-memory-observability";

describe("frontendMemorySnapshot", () => {
	it("reports query and live-buffer counts and approximate bytes", () => {
		const client = new QueryClient();
		client.setQueryData(["sessionThreads", "local", "s1"], { content: "abc" });
		const result = frontendMemorySnapshot(client, [{ sessionId: "s1", events: 2, bytes: 42 }]);
		expect(result.queries.count).toBe(1);
		expect(result.queries.byRoot.sessionThreads?.bytes).toBeGreaterThan(0);
		expect(result.liveEvents).toMatchObject({ sessions: 1, events: 2, bytes: 42 });
	});
});
