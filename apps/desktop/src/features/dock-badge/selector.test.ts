import { describe, expect, it } from "vitest";
import { selectUnreadSessionCount } from "./selector";

describe("selectUnreadSessionCount", () => {
	it("adds unread counts across workspaces", () => {
		expect(
			selectUnreadSessionCount([
				{ id: "a", name: "Alpha", branch: "main", status: "ready", unreadSessionCount: 1 },
				{ id: "b", name: "Beta", branch: "dev", status: "ready", unreadSessionCount: 3 },
			]),
		).toBe(4);
	});

	it("treats missing unread counts as zero", () => {
		expect(
			selectUnreadSessionCount([
				{ id: "a", name: "Alpha", branch: "main", status: "ready" },
				{ id: "b", name: "Beta", branch: "dev", status: "ready", unreadSessionCount: 2 },
			]),
		).toBe(2);
	});

	it("returns zero for an empty workspace list", () => {
		expect(selectUnreadSessionCount([])).toBe(0);
	});
});
