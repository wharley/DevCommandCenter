import { describe, expect, it } from "vitest";
import { buildProviderHandoffContext } from "./provider-handoff-context";
import { PendingForkReanchors, selectForkPoint } from "./fork-from-message.logic";
import type { WorkspaceMessage } from "./session-thread-history.logic";

function message(
	id: string,
	role: WorkspaceMessage["role"],
	content: string,
	extra: Partial<WorkspaceMessage> = {},
): WorkspaceMessage {
	return { id, role, label: role === "user" ? "User" : "Assistant", content, ...extra };
}

const thread: WorkspaceMessage[] = [
	message("u1", "user", "Add retries to checkout"),
	message("a1", "assistant", "Added a retry helper."),
	message("u2", "user", "Now cover the timeout case"),
	message("a2", "assistant", "Working…", { streaming: true }),
	message("u3", "user", "Also log failures"),
];

describe("selectForkPoint", () => {
	it("keeps only durable messages before the forked user message", () => {
		const point = selectForkPoint(thread, "u2");
		expect(point).not.toBeNull();
		expect(point?.priorMessages.map((entry) => entry.id)).toEqual(["u1", "a1"]);
		expect(point?.forkedPrompt).toBe("Now cover the timeout case");
		expect(point?.excludedUserTurns).toBe(2);
	});

	it("drops streaming or status messages from the snapshot and refuses unknown points", () => {
		const point = selectForkPoint(thread, "u3");
		expect(point?.priorMessages.map((entry) => entry.id)).toEqual(["u1", "a1", "u2"]);
		expect(selectForkPoint(thread, "missing")).toBeNull();
	});

	it("forks from an assistant reply keeping the pair and leaving the composer empty", () => {
		const point = selectForkPoint(thread, "a1");
		expect(point?.priorMessages.map((entry) => entry.id)).toEqual(["u1", "a1"]);
		expect(point?.forkedPrompt).toBe("");
		expect(point?.excludedUserTurns).toBe(2);
		// A streaming or incomplete reply is not a stable anchor.
		expect(selectForkPoint(thread, "a2")).toBeNull();
		expect(
			selectForkPoint(
				[message("u1", "user", "x"), message("a1", "assistant", "y", { status: { type: "incomplete" } })],
				"a1",
			),
		).toBeNull();
	});
});

describe("fork re-anchor", () => {
	it("labels the snapshot as a fork, excludes the forked prompt and stays bounded", () => {
		const point = selectForkPoint(thread, "u2");
		const context = buildProviderHandoffContext({
			mode: "fork",
			sourceProviderId: "codex",
			destinationProviderId: "claude_code",
			workspaceName: "shop",
			branch: "feat/retries",
			recentMessages: point?.priorMessages ?? [],
			currentPrompt: point?.forkedPrompt,
		});
		expect(context.startsWith("DCC fork re-anchor")).toBe(true);
		expect(context).toContain("not the source provider's native memory");
		expect(context).toContain("User: Add retries to checkout");
		expect(context).toContain("Assistant: Added a retry helper.");
		expect(context).not.toContain("Now cover the timeout case");
		expect(context).not.toContain("Also log failures");
		expect(context.length).toBeLessThanOrEqual(12_000);
	});

	it("is consumed exactly once per new session", () => {
		const pending = new PendingForkReanchors();
		pending.set("new-1", "snapshot");
		expect(pending.peek("new-1")).toBe("snapshot");
		expect(pending.peek("other")).toBeNull();
		expect(pending.consume("new-1")).toBe("snapshot");
		expect(pending.consume("new-1")).toBeNull();
		expect(pending.consume(null)).toBeNull();
	});
});
