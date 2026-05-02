import { describe, expect, it } from "vitest";
import { resolveCommitMode } from "./WorkspaceCommitButton.logic";

describe("resolveCommitMode", () => {
	it("defaults to create-pr when there is no stronger signal", () => {
		expect(resolveCommitMode("feature/statuslane")).toBe("create-pr");
	});

	it("keeps special branch-based modes", () => {
		expect(resolveCommitMode("fix/statuslane")).toBe("fix");
		expect(resolveCommitMode("release/push")).toBe("push");
		expect(resolveCommitMode("hotfix/merge")).toBe("merge");
		expect(resolveCommitMode("pr/statuslane")).toBe("open-pr");
	});
});
