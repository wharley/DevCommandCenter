import { describe, expect, it } from "vitest";
import { resolveActiveSkillsCheckout } from "./skills-path";

describe("resolveActiveSkillsCheckout", () => {
	it("uses the protected worktree when one exists", () => {
		expect(
			resolveActiveSkillsCheckout({
				rootPath: "/repo",
				worktreePath: "/dcc/worktrees/task",
			}),
		).toBe("/dcc/worktrees/task");
	});

	it("uses the repository root for local-direct workspaces", () => {
		expect(resolveActiveSkillsCheckout({ rootPath: "/repo", worktreePath: null })).toBe(
			"/repo",
		);
		expect(resolveActiveSkillsCheckout({ rootPath: "  ", worktreePath: null })).toBe(null);
	});
});
