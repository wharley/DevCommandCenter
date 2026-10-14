import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { BrowserApprovalHost } from "./browser-approval-host";

const mocks = vi.hoisted(() => ({ list: vi.fn(), resolve: vi.fn(), claim: vi.fn(), configure: vi.fn(), dismiss: vi.fn(), listen: vi.fn(), unlisten: vi.fn() }));
vi.mock("./browser-api", () => ({
	listAllBrowserOpenRequests: mocks.list,
	resolveBrowserOpenRequest: mocks.resolve,
	claimBrowserOpenRequest: mocks.claim,
}));
vi.mock("./browser-approval-panel", () => ({
	configureBrowserApprovalPanel: mocks.configure,
	dismissBrowserApprovalPanel: mocks.dismiss,
	onBrowserOpenApproved: mocks.listen,
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
	mocks.claim.mockImplementation(async () => request);
	mocks.configure.mockResolvedValue(undefined);
	mocks.dismiss.mockResolvedValue(undefined);
	mocks.listen.mockResolvedValue(mocks.unlisten);
	onApproved.mockReset();
	client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	container = document.createElement("div"); document.body.append(container); root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); client.clear(); container.remove(); vi.useRealTimers(); });
async function mount(sessions: WorkspaceSessionSummary[], visibleSessionId: string | null = null) {
	await act(async () => root.render(<QueryClientProvider client={client}><BrowserApprovalHost workspaceSessions={sessions} visibleSessionId={visibleSessionId} onApproved={onApproved} /></QueryClientProvider>));
	await act(async () => { await vi.advanceTimersByTimeAsync(10); });
}
async function render(sessions: WorkspaceSessionSummary[]) {
	await mount(sessions);
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

	it("opens a preapproved request at once when its task is on screen", async () => {
		const preapproved = { ...request, preapproved: true };
		mocks.list.mockResolvedValue([preapproved]);
		mocks.claim.mockImplementation(async () => preapproved);
		await mount([session("workspace-a", "session-a", "Target conversation")], "session-a");
		expect(mocks.claim).toHaveBeenCalledOnce();
		expect(onApproved).toHaveBeenCalledExactlyOnceWith(preapproved);
		expect(document.body.textContent).not.toContain(request.reason);
	});

	it("still asks for a preapproved request of a task the person is not looking at", async () => {
		mocks.list.mockResolvedValue([{ ...request, preapproved: true }]);
		await mount([session("workspace-a", "session-a", "Target conversation")], "other-session");
		expect(document.body.textContent).toContain(request.reason);
		expect(mocks.claim).not.toHaveBeenCalled();
	});

	it("does not deny when the card is dismissed with Escape", async () => {
		await render([session("workspace-a", "session-a", "Target conversation")]);
		await act(async () => {
			document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
		});
		expect(mocks.resolve).not.toHaveBeenCalled();
		expect(document.body.textContent).toContain(request.reason);
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

	it("claims before opening, so an answer from the phone wins", async () => {
		await render([]);
		mocks.claim.mockRejectedValueOnce(new Error("Esse pedido já foi negado pelo celular."));
		await act(async () => button("browser.approval.allow").click());
		expect(mocks.claim).toHaveBeenCalledWith({ requestId: request.requestId, workspaceId: request.workspaceId, sessionId: request.sessionId });
		expect(onApproved).not.toHaveBeenCalled();
		expect(document.body.textContent).toContain("Esse pedido já foi negado pelo celular.");
	});

	it("opens a request the panel or a phone approved, exactly once", async () => {
		await render([]);
		const approved = mocks.listen.mock.calls[0][0];
		await act(async () => { approved(request); approved(request); });
		expect(onApproved).toHaveBeenCalledExactlyOnceWith(request);
		expect(mocks.dismiss).toHaveBeenCalledExactlyOnceWith(request.requestId);
		expect(mocks.claim).not.toHaveBeenCalled();
		expect(mocks.resolve).not.toHaveBeenCalled();
		// An exit animation must not leave a portal hiding the native Browser
		// when the main window is minimized and no animation frames run.
		expect(document.querySelector("[data-dcc-browser-occluder]")).toBeNull();
	});

	it("ignores an outside approval of a request the modal already opened", async () => {
		await render([]);
		await act(async () => button("browser.approval.allow").click());
		await act(async () => mocks.listen.mock.calls[0][0](request));
		expect(onApproved).toHaveBeenCalledExactlyOnceWith(request);
	});

	it("opens an outside approval even when the hidden main window had not listed it", async () => {
		mocks.list.mockResolvedValue([]);
		await act(async () => root.render(<QueryClientProvider client={client}><BrowserApprovalHost workspaceSessions={[]} visibleSessionId={null} onApproved={onApproved} /></QueryClientProvider>));
		await act(async () => { await vi.advanceTimersByTimeAsync(10); });
		await act(async () => mocks.listen.mock.calls[0][0](request));
		expect(onApproved).toHaveBeenCalledExactlyOnceWith(request);
	});

	it("does not open from an outside approval after the host is unmounted", async () => {
		await render([]);
		const approved = mocks.listen.mock.calls[0][0];
		await act(async () => root.render(null));
		await act(async () => approved(request));
		expect(onApproved).not.toHaveBeenCalled();
		expect(mocks.unlisten).toHaveBeenCalledOnce();
	});
});
