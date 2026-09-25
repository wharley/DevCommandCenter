import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { BrowserApprovalHost } from "./browser-approval-host";

const mocks = vi.hoisted(() => ({ list: vi.fn(), resolve: vi.fn(), configure: vi.fn(), dismiss: vi.fn(), listen: vi.fn(), unlisten: vi.fn() }));
vi.mock("./browser-api", () => ({
	listAllBrowserOpenRequests: mocks.list,
	resolveBrowserOpenRequest: mocks.resolve,
}));
vi.mock("./browser-approval-panel", () => ({
	configureBrowserApprovalPanel: mocks.configure,
	dismissBrowserApprovalPanel: mocks.dismiss,
	onBrowserPanelAllow: mocks.listen,
}));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string, values?: { time?: string }) => values?.time ? `${key}:${values.time}` : key }),
}));

const request = { requestId: "bo-1", workspaceId: "workspace-a", sessionId: "session-a", providerId: "codex", url: "https://example.test", reason: "Need to inspect the page", expiresAtMs: 10_000_044_000 };
const session = (workspaceId: string, id: string, title: string) => ({ session: { workspaceId, id }, thread: { title } }) as unknown as WorkspaceSessionSummary;
let root: Root;
let client: QueryClient;
let container: HTMLDivElement;

beforeEach(() => {
	(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
	vi.clearAllMocks();
	vi.useFakeTimers();
	vi.setSystemTime(9_999_999_000);
	mocks.list.mockResolvedValue([request]);
	mocks.resolve.mockResolvedValue({ status: "denied", remainingMs: 0 });
	mocks.configure.mockResolvedValue(undefined);
	mocks.dismiss.mockResolvedValue(undefined);
	mocks.listen.mockResolvedValue(mocks.unlisten);
	onApproved.mockReset();
	client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	container = document.createElement("div"); document.body.append(container); root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); client.clear(); container.remove(); vi.useRealTimers(); });
async function render(sessions: WorkspaceSessionSummary[]) {
	await act(async () => root.render(<QueryClientProvider client={client}><BrowserApprovalHost workspaceSessions={sessions} onApproved={onApproved} /></QueryClientProvider>));
	await act(async () => { await vi.advanceTimersByTimeAsync(10); });
	expect(document.body.textContent).toContain(request.reason);
}
const onApproved = vi.fn();
const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((item) => item.textContent?.includes(text))!;

describe("Browser approval host", () => {
	it("approves once, selects the exact remote workspace/session, and leaves lifecycle ACK to the surface", async () => {
		await render([session("workspace-current", "other", "Current"), session("workspace-a", "session-a", "Target conversation")]);
		expect(document.body.textContent).toContain("Target conversation");
		await act(async () => button("browser.approval.allow").click());
		expect(onApproved).toHaveBeenCalledExactlyOnceWith(request);
		expect(mocks.resolve).not.toHaveBeenCalled();
	});

	it("denies without opening or acknowledging", async () => {
		await render([]);
		await act(async () => button("browser.approval.deny").click());
		expect(mocks.resolve).toHaveBeenCalledWith({ requestId: request.requestId, workspaceId: request.workspaceId, sessionId: request.sessionId, decision: "deny" });
		expect(onApproved).not.toHaveBeenCalled();
	});

	it("does not allow an expired request", async () => {
		mocks.list.mockResolvedValue([{ ...request, expiresAtMs: 9_999_999_100 }]);
		await render([]);
		await act(async () => { await vi.advanceTimersByTimeAsync(600); });
		expect(document.body.textContent).not.toContain(request.reason);
		expect(onApproved).not.toHaveBeenCalled();
		expect(mocks.resolve).not.toHaveBeenCalled();
	});

	it("ignores a repeated stale click after approval starts", async () => {
		await render([]);
		await act(async () => button("browser.approval.allow").click());
		await act(async () => { await vi.advanceTimersByTimeAsync(250); });
		expect(mocks.resolve).not.toHaveBeenCalledWith(expect.objectContaining({ decision: "allow" }));
		expect(onApproved).toHaveBeenCalledExactlyOnceWith(request);
	});

	it("re-enables the host for a second request after the first ACK removes it", async () => {
		await render([]);
		await act(async () => button("browser.approval.allow").click());
		const second = { ...request, requestId: "bo-2", url: request.url, expiresAtMs: 10_000_044_000 };
		mocks.list.mockResolvedValue([second]);
		await act(async () => { await vi.advanceTimersByTimeAsync(1_100); });
		expect(document.body.textContent).toContain("session-a");
		expect(button("browser.approval.allow").disabled).toBe(false);
	});

	it("refreshes an external approval and opens the exact request only once", async () => {
		await render([]);
		const externalAllow = mocks.listen.mock.calls[0][0];
		await act(async () => { externalAllow(request.requestId); externalAllow(request.requestId); });
		expect(onApproved).toHaveBeenCalledExactlyOnceWith(request);
		expect(mocks.dismiss).toHaveBeenCalledExactlyOnceWith(request.requestId);
		expect(mocks.resolve).not.toHaveBeenCalled();
		// An exit animation must not leave a portal hiding the native Browser
		// when the main window is minimized and no animation frames run.
		expect(document.querySelector("[data-dcc-browser-occluder]")).toBeNull();
	});

	it("does not approve a replacement request from a stale external click", async () => {
		await render([]);
		mocks.list.mockResolvedValue([{ ...request, requestId: "bo-next" }]);
		await act(async () => mocks.listen.mock.calls[0][0](request.requestId));
		expect(onApproved).not.toHaveBeenCalled();
	});

	it("does not approve an expired external request or use cached data after a refresh failure", async () => {
		await render([]);
		mocks.list.mockRejectedValueOnce(new Error("offline"));
		await act(async () => mocks.listen.mock.calls[0][0](request.requestId));
		expect(onApproved).not.toHaveBeenCalled();
		mocks.list.mockResolvedValue([{ ...request, expiresAtMs: Date.now() - 1 }]);
		await act(async () => mocks.listen.mock.calls[0][0](request.requestId));
		expect(onApproved).not.toHaveBeenCalled();
	});

	it("ignores an external click after the modal already started opening", async () => {
		await render([]);
		await act(async () => button("browser.approval.allow").click());
		await act(async () => mocks.listen.mock.calls[0][0](request.requestId));
		expect(onApproved).toHaveBeenCalledExactlyOnceWith(request);
	});

	it("can approve when the hidden main window had not discovered the request yet", async () => {
		mocks.list.mockResolvedValue([]);
		await act(async () => root.render(<QueryClientProvider client={client}><BrowserApprovalHost workspaceSessions={[]} onApproved={onApproved} /></QueryClientProvider>));
		await act(async () => { await vi.advanceTimersByTimeAsync(10); });
		mocks.list.mockResolvedValue([request]);
		await act(async () => mocks.listen.mock.calls[0][0](request.requestId));
		await act(async () => { await vi.advanceTimersByTimeAsync(10); });
		await act(async () => mocks.listen.mock.calls[0][0](request.requestId));
		expect(onApproved).toHaveBeenCalledExactlyOnceWith(request);
	});

	it("does not approve from an external event after the host is unmounted", async () => {
		await render([]);
		const externalAllow = mocks.listen.mock.calls[0][0];
		await act(async () => root.render(null));
		await act(async () => externalAllow(request.requestId));
		expect(onApproved).not.toHaveBeenCalled();
		expect(mocks.unlisten).toHaveBeenCalledOnce();
	});
});
