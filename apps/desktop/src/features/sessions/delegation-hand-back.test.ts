import { describe, expect, it } from "vitest";
import { parseDelegationHandBack } from "./delegation-hand-back";

describe("parseDelegationHandBack", () => {
	it("reads a finished or failed result turn", () => {
		expect(
			parseDelegationHandBack(
				"[DCC] Delegated implement task finished — Claude Code (claude-sonnet-5-5) (task c1fc267d-f53d).\n\nTask you delegated:\nFix it",
			),
		).toEqual({ delegationId: "c1fc267d-f53d", outcome: "finished" });
		expect(
			parseDelegationHandBack("[DCC] Delegated review task failed — Codex (task d-1).\n\nFailure: x"),
		).toEqual({ delegationId: "d-1", outcome: "failed" });
	});

	it("reads the applied and discarded outcome turns", () => {
		expect(
			parseDelegationHandBack(
				"[DCC] The human APPLIED the edits from your delegated implement task — Claude Code (task d-2).",
			),
		).toEqual({ delegationId: "d-2", outcome: "applied" });
		expect(
			parseDelegationHandBack(
				"[DCC] The human DISCARDED the edits from your delegated implement task — Codex (gpt-5.6) (task d-3).",
			),
		).toEqual({ delegationId: "d-3", outcome: "discarded" });
	});

	it("leaves prompts the person wrote alone", () => {
		expect(parseDelegationHandBack("[DCC] can you check this?")).toBeNull();
		expect(parseDelegationHandBack("Delegated implement task finished (task x).")).toBeNull();
	});
});
