import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { MenuBar } from "./MenuBar";
import * as api from "./api";
import "@/i18n/config";
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("./api", () => ({
	readMenuBar: vi.fn(), isMenuBarVisible: vi.fn(), hideMenuBar: vi.fn(),
	openFromMenuBar: vi.fn(), composeFromMenuBar: vi.fn(), quitFromMenuBar: vi.fn(),
}));
let root: Root;
let host: HTMLDivElement;
let visibility: (event: { payload: boolean }) => void;
const stop = vi.fn();
const task: api.MenuBarTask = { workspaceId: "w", sessionId: "s", title: "Fix the build", project: "DCC", status: "permission", updatedAt: "now" };
const snapshot: api.MenuBarSnapshot = { tasks: [task], metrics: { cpuPercent: null, memoryBytes: 1024 ** 3, processCount: 4 } };
beforeEach(() => {
	vi.useFakeTimers(); vi.clearAllMocks();
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	localStorage.setItem("dcc.ui.locale", "en");
	vi.mocked(listen).mockImplementation(async (_name, callback) => { visibility = callback as typeof visibility; return stop; });
	vi.mocked(api.isMenuBarVisible).mockResolvedValue(true);
	vi.mocked(api.readMenuBar).mockResolvedValue(snapshot);
	for (const method of [api.hideMenuBar, api.openFromMenuBar, api.composeFromMenuBar, api.quitFromMenuBar]) vi.mocked(method).mockResolvedValue();
	host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers(); vi.unstubAllGlobals(); });
const mount = async () => { await act(async () => root.render(<MenuBar />)); };
const click = async (label: string) => {
	const button = [...host.querySelectorAll("button")].find((button) => button.textContent?.includes(label))!;
	await act(async () => button.click());
};
it("shows pending tasks, opens the exact conversation and marks CPU warmup unavailable", async () => {
	await mount();
	expect(host.textContent).toContain("1 waiting for you");
	expect(host.textContent).toContain("Needs approval");
	expect(host.textContent).toContain("CPU —");
	expect(host.textContent).toContain("1 GiB");
	await click("Fix the build");
	expect(api.openFromMenuBar).toHaveBeenCalledWith(task);
});
it("polls only while visible, refreshes on reopen and cleans up subscriptions", async () => {
	await mount();
	await act(async () => vi.advanceTimersByTimeAsync(3000));
	expect(api.readMenuBar).toHaveBeenCalledTimes(2);
	await act(async () => visibility({ payload: false }));
	await act(async () => vi.advanceTimersByTimeAsync(15000));
	expect(api.readMenuBar).toHaveBeenCalledTimes(2);
	await act(async () => visibility({ payload: true }));
	expect(api.readMenuBar).toHaveBeenCalledTimes(3);
	await act(async () => root.unmount());
	expect(stop).toHaveBeenCalledTimes(1);
});
it("keeps failures distinct from an empty list and recovers", async () => {
	vi.mocked(api.readMenuBar).mockRejectedValueOnce(new Error("db locked"));
	await mount();
	expect(host.querySelector('[role="alert"]')).not.toBeNull();
	expect(host.textContent).not.toContain("No tasks running");
	await act(async () => vi.advanceTimersByTimeAsync(3000));
	expect(host.querySelector('[role="alert"]')).toBeNull();
	expect(host.textContent).toContain("Fix the build");
});
it("does not read activity for the initially hidden panel", async () => {
	vi.mocked(api.isMenuBarVisible).mockResolvedValue(false);
	await mount();
	await act(async () => vi.advanceTimersByTimeAsync(9000));
	expect(api.readMenuBar).not.toHaveBeenCalled();
	await act(async () => visibility({ payload: true }));
	expect(api.readMenuBar).toHaveBeenCalledTimes(1);
});
it("wires compose, main window, Escape and explicit quit", async () => {
	await mount();
	await click("New task"); expect(api.composeFromMenuBar).toHaveBeenCalledOnce();
	await click("Open DCC"); expect(api.openFromMenuBar).toHaveBeenCalledWith();
	await act(async () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
	expect(api.hideMenuBar).toHaveBeenCalledOnce();
	await click("Quit DCC"); expect(api.quitFromMenuBar).toHaveBeenCalledOnce();
});
it("discards an in-flight snapshot after hide/reopen and never overlaps reads", async () => {
	let resolve!: (value: api.MenuBarSnapshot) => void;
	vi.mocked(api.readMenuBar).mockReturnValueOnce(new Promise((done) => { resolve = done; }));
	await mount();
	await act(async () => visibility({ payload: false }));
	await act(async () => visibility({ payload: true }));
	expect(api.readMenuBar).toHaveBeenCalledTimes(1);
	await act(async () => resolve({ ...snapshot, tasks: [{ ...task, title: "Stale task" }] }));
	expect(host.textContent).not.toContain("Stale task");
	await act(async () => vi.advanceTimersByTimeAsync(1));
	expect(api.readMenuBar).toHaveBeenCalledTimes(2);
	expect(host.textContent).toContain("Fix the build");
});
