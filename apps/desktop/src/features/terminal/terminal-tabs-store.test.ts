import { beforeEach, describe, expect, it, vi } from "vitest";

const terminalStore = vi.hoisted(() => ({
	disposeTerminal: vi.fn(),
}));

vi.mock("./terminal-store", () => ({
	disposeTerminal: terminalStore.disposeTerminal,
}));

describe("terminal tab names", () => {
	const values = new Map<string, string>();

	beforeEach(() => {
		vi.resetModules();
		terminalStore.disposeTerminal.mockReset();
		values.clear();
		vi.stubGlobal("localStorage", {
			getItem: (key: string) => values.get(key) ?? null,
			setItem: (key: string, value: string) => values.set(key, value),
		});
	});

	it("disposes the runtime when a tab is permanently closed", async () => {
		const { addTerminal, getTerminalRuntimeId, removeTerminal } = await import(
			"./terminal-tabs-store"
		);
		const tabId = addTerminal("project-1");

		removeTerminal("project-1", tabId!);

		expect(terminalStore.disposeTerminal).toHaveBeenCalledWith(
			getTerminalRuntimeId("project-1", tabId!),
		);
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

	it("does not reuse a busy tab when a caller requires a fresh terminal", async () => {
		const { addTerminal, MAX_TERMINAL_TABS } = await import(
			"./terminal-tabs-store"
		);
		for (let index = 0; index < MAX_TERMINAL_TABS; index += 1) {
			expect(addTerminal("project-1")).not.toBeNull();
		}

		expect(
			addTerminal("project-1", { reuseAtCapacity: false }),
		).toBeNull();
	});
});
