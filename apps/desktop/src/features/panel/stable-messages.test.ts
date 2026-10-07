import { describe, expect, it } from "vitest";
import type { WorkspaceMessage } from "./thread-projection";
import { sameProjectionValue, stabilizeMessages } from "./stable-messages";

function message(id: string, content: string, extra: Partial<WorkspaceMessage> = {}): WorkspaceMessage {
	return { id, role: "assistant", label: "Assistant", content, ...extra };
}

describe("stable messages", () => {
	it("reuses unchanged message objects across projections", () => {
		const first = stabilizeMessages(new Map(), [
			message("a", "done", { annotations: [{ type: "commentary", id: "c", content: "x" }] }),
			message("b", "stream"),
		]);
		const second = stabilizeMessages(first.byId, [
			message("a", "done", { annotations: [{ type: "commentary", id: "c", content: "x" }] }),
			message("b", "streaming more"),
		]);
		expect(second.messages[0]).toBe(first.messages[0]);
		expect(second.messages[1]).not.toBe(first.messages[1]);
		expect(second.changed).toBe(true);
	});

	it("compares nested projection values structurally", () => {
		expect(sameProjectionValue({ a: [1, { b: "c" }] }, { a: [1, { b: "c" }] })).toBe(true);
		expect(sameProjectionValue({ a: [1] }, { a: [1, 2] })).toBe(false);
		expect(sameProjectionValue({ a: undefined }, {})).toBe(false);
	});
});
