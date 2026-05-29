import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Fase 0 guard: project-scoped skills only work because the sidecar passes
 * `settingSources` including "project" to the Claude Agent SDK. If that ever
 * regresses to e.g. ["user"], `.claude/skills/` in the worktree stop loading
 * silently. This test fails loudly if "project" is dropped.
 */
describe("sidecar settingSources", () => {
	it("includes 'project' so worktree .claude/skills are discovered", () => {
		const sidecarPath = path.resolve(
			process.cwd(),
			"../../sidecar/src/index.mjs",
		);
		const source = readFileSync(sidecarPath, "utf8");
		const match = source.match(/settingSources:\s*\[([^\]]*)\]/);
		expect(match, "settingSources array not found in sidecar").toBeTruthy();
		expect(match?.[1]).toContain('"project"');
	});
});
