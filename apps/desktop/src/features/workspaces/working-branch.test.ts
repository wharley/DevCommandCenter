import { describe, expect, it } from "vitest";
import { workingBranchLabel } from "./working-branch";

const t = ((key: string, options?: Record<string, string>) =>
	options ? `${key}:${options.branch}` : key) as never;

describe("workingBranchLabel", () => {
	it("names a real branch as it is", () => {
		expect(workingBranchLabel("feat/x", "main", t)).toBe("feat/x");
	});

	it("says a detached checkout has no branch, next to its base", () => {
		expect(workingBranchLabel("HEAD", "main", t)).toBe("workingBranch.detached:main");
		expect(workingBranchLabel("HEAD", null, t)).toBe("workingBranch.detachedNoBase");
		expect(workingBranchLabel("HEAD", "HEAD", t)).toBe("workingBranch.detachedNoBase");
	});

	it("falls back to the base while the status is unknown", () => {
		expect(workingBranchLabel(null, "main", t)).toBe("main");
		expect(workingBranchLabel(undefined, null, t)).toBeNull();
	});
});
