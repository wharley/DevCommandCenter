import { describe, expect, it } from "vitest";
import { findInThread, stepThreadFindIndex } from "./thread-find.logic";
import type { WorkspaceMessage } from "./thread-projection";

function message(
	id: string,
	role: WorkspaceMessage["role"],
	content: string,
	extra: Partial<WorkspaceMessage> = {},
): WorkspaceMessage {
	return { id, role, label: role, content, ...extra };
}

const thread = [
	message("u1", "user", "Add retries to the Checkout flow"),
	message("a1", "assistant", "I added a retry helper in checkout.ts and wired it."),
	message("s1", "system", "session.resumed"),
	message("u2", "user", "Now the timeout case"),
	message("a2", "assistant", "checkout retry streaming…", { streaming: true }),
];

describe("findInThread", () => {
	it("matches case-insensitively in timeline order and skips streaming messages", () => {
		const matches = findInThread(thread, "checkout");
		expect(matches.map((match) => match.messageId)).toEqual(["u1", "a1"]);
		expect(matches[0].role).toBe("user");
		expect(matches[1].snippet).toContain("checkout.ts");
	});

	it("ignores queries that are too short or blank", () => {
		expect(findInThread(thread, "c")).toEqual([]);
		expect(findInThread(thread, "   ")).toEqual([]);
		expect(findInThread(thread, "nothing-here")).toEqual([]);
	});

	it("bounds snippets around the first occurrence", () => {
		const long = `${"x".repeat(300)} needle ${"y".repeat(300)}`;
		const [match] = findInThread([message("m", "assistant", long)], "needle");
		expect(match.snippet.length).toBeLessThanOrEqual(100);
		expect(match.snippet.startsWith("…")).toBe(true);
		expect(match.snippet.endsWith("…")).toBe(true);
		expect(match.snippet).toContain("needle");
	});
});

describe("stepThreadFindIndex", () => {
	it("wraps in both directions and tolerates an empty list", () => {
		expect(stepThreadFindIndex(0, 3, 1)).toBe(1);
		expect(stepThreadFindIndex(2, 3, 1)).toBe(0);
		expect(stepThreadFindIndex(0, 3, -1)).toBe(2);
		expect(stepThreadFindIndex(5, 0, 1)).toBe(0);
	});
});
