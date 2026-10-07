import { describe, expect, it } from "vitest";
import type { AssistantActivityAnnotation } from "./assistant-activity-disclosure";
import {
	classifyToolAction,
	diffStats,
	formatElapsed,
	formatTokenCount,
	mcpToolLabel,
	segmentTurn,
	stripAnsi,
	summarizeTurnWork,
	toolTarget,
	totalTurnTokens,
	turnDurationMs,
} from "./turn-activity.logic";
import { nextPacedLength } from "./use-paced-text";

function tool(
	id: string,
	action: string,
	extra: Partial<Extract<AssistantActivityAnnotation, { type: "tool-call" }>> = {},
): AssistantActivityAnnotation {
	return { type: "tool-call", id, action, content: "", ...extra };
}

function prose(id: string, content: string): AssistantActivityAnnotation {
	return { type: "commentary", id, content };
}

describe("turn activity", () => {
	it("classifies provider tool names into summary kinds", () => {
		expect(classifyToolAction("Bash")).toBe("command");
		expect(classifyToolAction("apply_patch")).toBe("edit");
		expect(classifyToolAction("MultiEdit")).toBe("edit");
		expect(classifyToolAction("Grep")).toBe("search");
		expect(classifyToolAction("mcp__github__create_issue")).toBe("mcp");
		expect(classifyToolAction("SomethingNew")).toBe("other");
		expect(mcpToolLabel("mcp__github__create_issue")).toBe("github · create_issue");
	});

	it("keeps execution order: prose between bursts of work", () => {
		const segments = segmentTurn([
			tool("t1", "Read"),
			tool("t2", "Grep"),
			prose("c1", "Found it."),
			{ type: "reasoning", id: "r1", content: "hmm" },
			tool("t3", "Edit"),
			prose("c2", "   "),
		]);
		expect(segments.map((segment) => segment.type)).toEqual(["work", "prose", "work"]);
		expect(segments[0]?.type === "work" && segments[0].items).toHaveLength(2);
		expect(segments[2]?.type === "work" && segments[2].items).toHaveLength(2);
	});

	it("summarizes work by kind and counts edited files once", () => {
		const summary = summarizeTurnWork([
			tool("t1", "Edit", { file: "src/a.ts" }),
			tool("t2", "Edit", { file: "src/a.ts" }),
			tool("t3", "Write", { file: "src/b.ts" }),
			tool("t4", "Bash", { status: { type: "failed" } }),
			{ type: "reasoning", id: "r1", content: "" },
		]);
		expect(summary.editedFiles).toBe(2);
		expect(summary.counts.command).toBe(1);
		expect(summary.failures).toBe(1);
		expect(summary.thoughts).toBe(1);
	});

	it("uses the concrete command or file as the row target", () => {
		const bash = tool("t1", "Bash", { detail: { command: "npm test -- --watch=false\nmore" } });
		expect(bash.type === "tool-call" && toolTarget(bash)).toBe("npm test -- --watch=false");
		const edit = tool("t2", "Edit", { file: "/repo/src/features/a.ts" });
		expect(edit.type === "tool-call" && toolTarget(edit)).toBe("features/a.ts");
	});

	it("counts diff additions and deletions without headers", () => {
		expect(diffStats("--- a/x\n+++ b/x\n@@\n-old\n+new\n+more\n")).toEqual({
			additions: 2,
			deletions: 1,
		});
		expect(diffStats("")).toBeNull();
	});

	it("formats durations and token counts compactly", () => {
		expect(formatElapsed(12_400)).toBe("12s");
		expect(formatElapsed(72_000)).toBe("1m 12s");
		expect(formatElapsed(3_780_000)).toBe("1h 03m");
		expect(turnDurationMs("2026-05-01T12:00:00Z", "2026-05-01T12:01:12Z")).toBe(72_000);
		expect(turnDurationMs("2026-05-01T12:00:00Z", undefined)).toBeNull();
		expect(formatTokenCount(950)).toBe("950");
		expect(formatTokenCount(12_345)).toBe("12k");
		expect(formatTokenCount(1_500)).toBe("1.5k");
	});

	it("totals exact usage across models", () => {
		const totals = totalTurnTokens([
			{
				model: "a",
				inputTokens: 100,
				outputTokens: 20,
				cachedInputTokens: 50,
				cacheWriteInputTokens: 0,
				reasoningOutputTokens: 5,
				totalTokens: 120,
				costUsd: 0.01,
			},
			{
				model: "b",
				inputTokens: 10,
				outputTokens: 2,
				cachedInputTokens: 0,
				cacheWriteInputTokens: 0,
				reasoningOutputTokens: 0,
				totalTokens: 0,
				costUsd: null,
			},
		]);
		expect(totals).toMatchObject({ input: 110, output: 22, cached: 50, total: 132, costUsd: 0.01 });
		expect(totalTurnTokens([])).toBeNull();
	});

	it("strips ANSI colors from captured output", () => {
		expect(stripAnsi("\u001b[31mfail\u001b[0m ok")).toBe("fail ok");
	});

	it("paces streamed text by backlog and does not stop mid-word", () => {
		const target = "hello wonderful world";
		const next = nextPacedLength(0, target, 16);
		expect(next).toBeGreaterThan(0);
		expect(next).toBeLessThanOrEqual(target.length);
		if (next < target.length) expect(target[next]).toMatch(/\s/);
		expect(nextPacedLength(target.length, target, 16)).toBe(target.length);
		// A large backlog catches up quickly.
		const long = "x ".repeat(2000);
		expect(nextPacedLength(0, long, 100)).toBeGreaterThan(150);
	});
});
