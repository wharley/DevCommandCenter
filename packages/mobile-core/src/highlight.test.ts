import { describe, expect, it } from "vitest";
import { projectDiffStat, workspaceDiffStat, type DeliverySummary } from "./delivery.ts";
import { highlightCode } from "./highlight.ts";

describe("highlightCode", () => {
	it("colors keywords, strings, numbers and comments", () => {
		const tokens = highlightCode('const a = "x"; // note\nreturn 42', "ts");
		expect(tokens.filter((t) => t.kind !== "plain")).toEqual([
			{ kind: "keyword", text: "const" },
			{ kind: "string", text: '"x"' },
			{ kind: "comment", text: "// note" },
			{ kind: "keyword", text: "return" },
			{ kind: "number", text: "42" },
		]);
	});

	it("keeps every character, in order", () => {
		const code = "def f(x):\n    # hi\n    return x * 2.5 /* odd */ 'unterminated";
		expect(highlightCode(code, "py").map((t) => t.text).join("")).toBe(code);
	});

	it("uses # comments only for languages that have them", () => {
		expect(highlightCode("# title", "py")[0]).toEqual({ kind: "comment", text: "# title" });
		expect(highlightCode("a # b", "ts").some((t) => t.kind === "comment")).toBe(false);
	});
});

describe("workspaceDiffStat", () => {
	const change = (path: string, insertions: number, deletions: number) => ({ path, status: "M", insertions, deletions, staged: false });
	it("counts a file once across branch and local changes", () => {
		const summary = {
			branch: "feat/x",
			changes: [change("a.ts", 3, 1), change("b.ts", 2, 0)],
			branchChanges: [change("a.ts", 10, 4)],
		} as unknown as DeliverySummary;
		expect(workspaceDiffStat(summary)).toEqual({ branch: "feat/x", files: 2, additions: 15, deletions: 5 });
	});
});

describe("projectDiffStat", () => {
	it("adds up worktrees and names the branch only when there is one", () => {
		const a = { branch: "feat/a", files: 2, additions: 10, deletions: 1 };
		const b = { branch: "main", files: 1, additions: 3, deletions: 4 };
		expect(projectDiffStat([a, b])).toEqual({ worktrees: 2, files: 3, additions: 13, deletions: 5, branch: null });
		expect(projectDiffStat([a])).toEqual({ worktrees: 1, files: 2, additions: 10, deletions: 1, branch: "feat/a" });
	});
});
