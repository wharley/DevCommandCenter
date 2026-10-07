import { describe, expect, it } from "vitest";
import { delegationVerifications, validationSegment } from "./delegation-verification";
import type { WorkspaceMessage } from "./session-thread-history.logic";

function toolCall(id: string, command: string, exitCode: number | null, failed = false) {
	return {
		type: "tool-call" as const,
		id,
		action: "commandExecution",
		command,
		content: "",
		detail: { command, exitCode },
		...(failed ? { status: { type: "failed" as const } } : {}),
	};
}

describe("validationSegment", () => {
	it("keeps the validating part of a shell line", () => {
		expect(validationSegment("cd /tmp/x && swift test 2>&1 | tail -20")).toBe("swift test");
		expect(validationSegment("yarn vitest run src/a.test.ts")).toBe("yarn vitest run src/a.test.ts");
		expect(validationSegment("git status --short")).toBeNull();
	});
});

describe("delegationVerifications", () => {
	const handBack: WorkspaceMessage = {
		id: "u-2",
		role: "user",
		turnId: "turn-2",
		label: "User",
		content: "[DCC] …",
		delegationHandBack: { delegationId: "d-1", outcome: "finished" },
	};

	it("reads the checks of the turn the hand-back started, latest run winning", () => {
		const reply: WorkspaceMessage = {
			id: "a-2",
			role: "assistant",
			turnId: "turn-2",
			turnSettled: true,
			label: "Assistant",
			content: "Validated.",
			annotations: [
				toolCall("c1", "git diff --stat", 0),
				toolCall("c2", "swift build", 0),
				toolCall("c3", "swift test", 1, true),
				toolCall("c4", "swift test", 0),
			],
		};
		expect(delegationVerifications([handBack, reply]).get("d-1")).toEqual([
			{ command: "swift build", ok: true },
			{ command: "swift test", ok: true },
		]);
	});

	it("waits for the turn to settle and ignores other turns", () => {
		const live: WorkspaceMessage = {
			id: "a-2",
			role: "assistant",
			turnId: "turn-2",
			turnSettled: false,
			label: "Assistant",
			content: "",
			annotations: [toolCall("c1", "swift test", 0)],
		};
		const other: WorkspaceMessage = { ...live, id: "a-1", turnId: "turn-1", turnSettled: true };
		expect(delegationVerifications([handBack, live, other]).size).toBe(0);
	});
});
