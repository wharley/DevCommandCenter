import { describe, expect, it } from "vitest";
import {
	authorSessionId,
	buildFixRequest,
	findingSnippet,
	type FindingForAuthor,
} from "./finding-to-author-command";

const labels = {
	one: "Fix this Reviewer finding",
	many: "Fix these 2 Reviewer findings",
	location: (start: number, end: number) => (start === end ? `line ${start}` : `lines ${start}–${end}`),
};

const finding = (overrides: Partial<FindingForAuthor> = {}): FindingForAuthor => ({
	path: "src/math.ts",
	startLine: 3,
	endLine: 13,
	title: "divide has no regression tests",
	detail: "Add tests for zero, -0 and overflow so the guards cannot be removed silently.",
	snippet: "",
	...overrides,
});

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

	it("sends the reviewer's explanation, not just the title", () => {
		expect(buildFixRequest([finding({ snippet: "return a / b;" })], labels)).toBe(
			[
				"Fix this Reviewer finding:",
				"",
				"`src/math.ts` (lines 3–13): divide has no regression tests",
				"Add tests for zero, -0 and overflow so the guards cannot be removed silently.",
				"",
				"```ts startLine=3",
				"return a / b;",
				"```",
			].join("\n"),
		);
		// No snippet at hand and no detail: location and title only.
		expect(buildFixRequest([finding({ detail: "", startLine: 3, endLine: 3 })], labels)).toBe(
			"Fix this Reviewer finding:\n\n`src/math.ts` (line 3): divide has no regression tests",
		);
	});

	it("numbers several findings in one request", () => {
		expect(
			buildFixRequest(
				[finding(), finding({ path: "src/b.ts", startLine: 7, endLine: 7, title: "Null deref", detail: "" })],
				labels,
			),
		).toBe(
			[
				"Fix these 2 Reviewer findings:",
				"",
				"1. `src/math.ts` (lines 3–13): divide has no regression tests",
				"   Add tests for zero, -0 and overflow so the guards cannot be removed silently.",
				"2. `src/b.ts` (line 7): Null deref",
			].join("\n"),
		);
	});

	it("ends the request by keeping the fix to what was pointed out", () => {
		const scoped = { ...labels, scope: "Make the smallest change that fixes it." };
		expect(buildFixRequest([finding({ detail: "" })], scoped)).toBe(
			"Fix this Reviewer finding:\n\n`src/math.ts` (lines 3–13): divide has no regression tests\n\nMake the smallest change that fixes it.",
		);
		expect(buildFixRequest([finding(), finding()], scoped).endsWith("\n\nMake the smallest change that fixes it.")).toBe(true);
	});
});
