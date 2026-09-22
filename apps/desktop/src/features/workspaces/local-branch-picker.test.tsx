import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { LocalBranchPicker } from "./local-branch-picker";
import { workspaceLocalBranches, workspaceSwitchLocalBranch } from "@/lib/workspace-api";
import type { LocalBranchesOutput } from "@dcc/contracts";

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("@/lib/workspace-api", () => ({ workspaceLocalBranches: vi.fn(), workspaceSwitchLocalBranch: vi.fn() }));
let root: Root;
let container: HTMLDivElement;
let client: QueryClient;
const onBusyChange = vi.fn();
const initial: LocalBranchesOutput = {
	currentBranch: "feature/current", taskBranch: "main", hasConversation: false, agentRunning: false, refreshError: null,
	branches: [
		{ name: "main", reference: "refs/heads/main", remote: false },
		{ name: "feature/current", reference: "refs/heads/feature/current", remote: false },
		{ name: "origin/incoming", reference: "refs/remotes/origin/incoming", remote: true },
	],
};
async function settle() { await act(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); }); }
async function render(conversationStarted = false, busy = false) {
	await act(async () => root.render(<QueryClientProvider client={client}><LocalBranchPicker workspaceId="task" fallbackBranch="main" conversationStarted={conversationStarted} busy={busy} onBusyChange={onBusyChange} /></QueryClientProvider>));
	await settle();
}
function trigger() { return container.querySelector("button") as HTMLButtonElement; }
function button(text: string) { return [...document.querySelectorAll("button")].find((node) => node.textContent === text) as HTMLButtonElement; }
async function click(node: HTMLElement) { await act(async () => node.click()); await settle(); }

beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
	Element.prototype.scrollIntoView = vi.fn();
	vi.mocked(workspaceLocalBranches).mockReset().mockResolvedValue(structuredClone(initial));
	vi.mocked(workspaceSwitchLocalBranch).mockReset();
	onBusyChange.mockReset();
	client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
	container = document.createElement("div"); document.body.appendChild(container); root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); client.clear(); container.remove(); vi.unstubAllGlobals(); });

it("shows the actual branch, lists local and remote refs, and switches explicitly", async () => {
	vi.mocked(workspaceSwitchLocalBranch).mockResolvedValue({ ...initial, currentBranch: "incoming", taskBranch: "incoming" });
	await render();
	expect(trigger().textContent).toBe("feature/current");
	await click(trigger());
	expect(workspaceLocalBranches).toHaveBeenCalledWith({ workspaceId: "task", refresh: true });
	expect(button("main")).toBeTruthy();
	await click(button("origin/incoming"));
	expect(workspaceSwitchLocalBranch).toHaveBeenCalledWith({ workspaceId: "task", expectedBranch: "feature/current", reference: "refs/remotes/origin/incoming", newBranch: null });
	expect(onBusyChange).toHaveBeenCalledWith(true);
	expect(onBusyChange).toHaveBeenLastCalledWith(false);
});

it("closes an open picker as soon as the first message appears", async () => {
	await render(); await click(trigger()); expect(document.querySelector('[role="dialog"]')).not.toBeNull();
	await render(true); expect(trigger().disabled).toBe(true);
	expect(trigger().title).toBe("localBranch.locked");
	expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("keeps a reopened conversation locked even if its selected timeline is empty", async () => {
	vi.mocked(workspaceLocalBranches).mockResolvedValue({ ...initial, hasConversation: true, taskBranch: "feature/current" });
	await render(); expect(trigger().disabled).toBe(true);
	expect(workspaceSwitchLocalBranch).not.toHaveBeenCalled();
});

it("blocks checkout while an agent in another task is running", async () => {
	vi.mocked(workspaceLocalBranches).mockResolvedValue({ ...initial, agentRunning: true });
	await render(); expect(trigger().disabled).toBe(true); expect(trigger().title).toBe("localBranch.agentRunning");
});

it("keeps known branches usable when the background fetch fails", async () => {
	vi.mocked(workspaceLocalBranches).mockImplementation(async (input) => ({ ...initial, refreshError: input.refresh ? "offline" : null }));
	await render(); await click(trigger());
	expect(document.body.textContent).toContain("localBranch.refreshFailed");
	expect(button("main").disabled).toBe(false);
});

it("warns when an external checkout changes the branch of an existing conversation", async () => {
	vi.mocked(workspaceLocalBranches).mockResolvedValue({ ...initial, hasConversation: true });
	await render(); expect(container.querySelector('[role="alert"]')?.textContent).toBe("localBranch.mismatch");
	expect(trigger().disabled).toBe(true);
});
