import { describe, expect, it } from "vitest";
import { SESSION_EVENT_NAMES } from "./session-api";

describe("session event subscriptions", () => {
	it("subscribes to every model event that can enrich a live subagent card", () => {
		expect(SESSION_EVENT_NAMES).toEqual(
			expect.arrayContaining([
				"dcc/session/turn/native-subagent/activity",
				"dcc/session/turn/native-subagent/model-requested",
				"dcc/session/turn/native-subagent/model-confirmed",
				"dcc/session/turn/model-effective",
			]),
		);
	});
});
