import { describe, expect, it } from "vitest";
import { authorSessionId, findingSnippet } from "./finding-to-author-command";

describe("finding to author", () => {
	it("quotes the lines the finding points at", () => {
		const text = "one\ntwo\nthree\nfour";
		expect(findingSnippet(text, 2, 3)).toBe("two\nthree");
		expect(findingSnippet(text, 4, 4)).toBe("four");
		// A range past the end of the file quotes what exists.
		expect(findingSnippet(text, 4, 9)).toBe("four");
	});

	it("picks the most recently used plain session, never an agent session", () => {
		const sessions = [
			{ session: { id: "old" }, lastTurnStartedAt: "2026-10-01T10:00:00Z" },
			{ session: { id: "reviewer" }, lastTurnStartedAt: "2026-10-01T12:00:00Z" },
			{ session: { id: "author" }, lastTurnStartedAt: "2026-10-01T11:00:00Z" },
		];
		expect(authorSessionId(sessions, (id) => id === "reviewer")).toBe("author");
		expect(authorSessionId(sessions.slice(1, 2), (id) => id === "reviewer")).toBeNull();
		expect(authorSessionId([{ session: { id: "fresh" } }], () => false)).toBe("fresh");
	});
});
