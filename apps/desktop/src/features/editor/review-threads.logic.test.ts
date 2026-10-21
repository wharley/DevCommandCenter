import type { WorkspacePrReviewComment } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import {
	buildReviewThreads,
	countOpenReviewThreads,
	reviewThreadAnchorable,
	reviewThreadRoot,
	reviewThreadState,
	sortReviewThreadsForList,
} from "./review-threads.logic";

function comment(overrides: Partial<WorkspacePrReviewComment>): WorkspacePrReviewComment {
	return {
		id: "1",
		parentId: null,
		threadId: "t1",
		path: "src/app.ts",
		body: "Please rename.",
		diffHunk: null,
		htmlUrl: null,
		side: "RIGHT",
		line: 10,
		startLine: null,
		originalLine: 10,
		originalStartLine: null,
		author: { login: "alice", avatarUrl: null, htmlUrl: null },
		createdAt: "2026-10-01T10:00:00Z",
		updatedAt: null,
		resolvable: true,
		resolved: false,
		outdated: false,
		...overrides,
	};
}

describe("review threads", () => {
	it("puts the root first even when a reply carries an earlier timestamp", () => {
		const [thread] = buildReviewThreads([
			comment({ id: "2", parentId: "1", createdAt: "2026-09-01T00:00:00Z" }),
			comment({ id: "1" }),
		]);
		expect(thread!.comments.map((entry) => entry.id)).toEqual(["1", "2"]);
		expect(reviewThreadRoot(thread!).id).toBe("1");
	});

	it("tells open, outdated and resolved apart", () => {
		const threads = buildReviewThreads([
			comment({ threadId: "open", line: 3 }),
			comment({ threadId: "outdated", outdated: true, line: 4 }),
			// REST fallback: GitHub drops `line` once the code changed.
			comment({ threadId: "gone", line: null, startLine: null, originalLine: null, outdated: null }),
			comment({ threadId: "resolved", resolved: true, line: 6 }),
		]);
		const byId = (id: string) => threads.find((thread) => thread.id === id)!;
		const [open, outdated, gone, resolved] = ["open", "outdated", "gone", "resolved"].map(byId);
		expect(reviewThreadState(open!)).toBe("open");
		expect(reviewThreadState(outdated!)).toBe("outdated");
		expect(reviewThreadState(gone!)).toBe("outdated");
		expect(reviewThreadState(resolved!)).toBe("resolved");
	});

	it("draws only open threads, and only on the pull request diff", () => {
		const [open, resolved] = buildReviewThreads([
			comment({ threadId: "open", line: 3 }),
			comment({ threadId: "resolved", resolved: true, line: 6 }),
		]);
		expect(reviewThreadAnchorable(open!, true)).toBe(true);
		expect(reviewThreadAnchorable(open!, false)).toBe(false);
		expect(reviewThreadAnchorable(resolved!, true)).toBe(false);
	});

	it("lists open conversations first and counts what still needs attention", () => {
		const comments = [
			comment({ threadId: "resolved", resolved: true, line: 1 }),
			comment({ threadId: "late", line: 40 }),
			comment({ threadId: "outdated", outdated: true, line: 2 }),
			comment({ threadId: "early", line: 5 }),
		];
		expect(
			sortReviewThreadsForList(buildReviewThreads(comments)).map((thread) => thread.id),
		).toEqual(["early", "late", "outdated", "resolved"]);
		expect(countOpenReviewThreads(comments)).toBe(3);
	});
});
