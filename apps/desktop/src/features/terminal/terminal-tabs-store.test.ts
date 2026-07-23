import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./terminal-store", () => ({
	killTerminal: vi.fn(),
}));

describe("terminal tab names", () => {
	const values = new Map<string, string>();

	beforeEach(() => {
		vi.resetModules();
		values.clear();
		vi.stubGlobal("localStorage", {
			getItem: (key: string) => values.get(key) ?? null,
			setItem: (key: string, value: string) => values.set(key, value),
		});
	});

	it("persists a renamed tab without changing its identity", async () => {
		const { addTerminal, renameTerminal } = await import("./terminal-tabs-store");
		const tabId = addTerminal("project-1");

		expect(tabId).not.toBeNull();
		expect(renameTerminal("project-1", tabId!, "API server")).toBe(true);

		const stored = JSON.parse(values.get("dcc-terminal-tabs-v1") ?? "{}") as Record<
			string,
			{ tabs: Array<{ id: string; title: string }> }
		>;
		expect(stored["project-1"].tabs).toEqual([{ id: tabId, title: "API server" }]);
	});

	it("rejects an empty tab name", async () => {
		const { addTerminal, renameTerminal } = await import("./terminal-tabs-store");
		const tabId = addTerminal("project-1");

		expect(renameTerminal("project-1", tabId!, "   ")).toBe(false);
	});
});
