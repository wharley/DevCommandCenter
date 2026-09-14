import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { ComputerUseApprovalHost } from "./computer-use-approval-host";

const mocks = vi.hoisted(() => ({
	list: vi.fn(),
	status: vi.fn(),
	access: vi.fn(),
	respond: vi.fn(),
	toastInfo: vi.fn(),
}));

vi.mock("@/lib/computer-use-api", () => ({
	listComputerUsePendingRequests: mocks.list,
	getComputerUseStatus: mocks.status,
	requestComputerUseAccess: mocks.access,
	respondComputerUseControlRequest: mocks.respond,
}));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: { id?: string; time?: string }) =>
			values?.id ? `${key}:${values.id}` : values?.time ? `${key}:${values.time}` : key,
	}),
}));
vi.mock("sonner", () => ({ toast: { info: mocks.toastInfo } }));

const request = {
	requestId: "request-1",
	sessionId: "session-1",
	workspaceId: "workspace-1",
	providerId: "codex",
	reason: "Open the selected app",
	remainingMs: 45_000,
};
const status = {
	platform: "macos",
	supported: true,
	unsupportedReason: null,
	minimumMacosVersion: 14,
	providerSupported: true,
	runtimeAttached: true,
	accessibility: { granted: true, canRequest: false },
	screenRecording: { granted: true, canRequest: false },
	grant: { sessionId: "session-1", armed: false, remainingMs: 0, allowedBundleIds: [] },
	targets: [{ bundleId: "com.apple.Safari", name: "Safari", pid: 12, windowId: 5, title: "Docs" }],
};
const session = {
	session: { id: "session-1", workspaceId: "workspace-1", providerId: "codex" },
	thread: { title: "Review docs" },
} as unknown as WorkspaceSessionSummary;

let root: Root;
let container: HTMLDivElement;
let client: QueryClient;

beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	vi.clearAllMocks();
	mocks.list.mockResolvedValue([request]);
	mocks.status.mockResolvedValue(status);
	mocks.access.mockResolvedValue(status);
	mocks.respond.mockResolvedValue({ grant: status.grant });
	client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});

afterEach(async () => {
	await act(async () => root.unmount());
	client.clear();
	container.remove();
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

async function render(sessions: WorkspaceSessionSummary[] = []) {
	await act(async () => {
		root.render(
			<QueryClientProvider client={client}>
				<ComputerUseApprovalHost workspaceSessions={sessions} />
			</QueryClientProvider>,
		);
	});
	await vi.waitFor(() => expect(document.body.textContent).toContain(request.reason));
}

function button(text: string): HTMLButtonElement {
	const found = [...document.querySelectorAll<HTMLButtonElement>("button")]
		.find((element) => element.textContent?.includes(text));
	expect(found).toBeDefined();
	return found!;
}

describe("Computer Use approval host", () => {
	it("shows the matching conversation and sends only the selected app when allowed", async () => {
		await render([session]);
		expect(document.body.textContent).toContain("Review docs");
		await vi.waitFor(() => expect(document.querySelector<HTMLInputElement>('input[type="checkbox"]')).not.toBeNull());
		const checkbox = document.querySelector<HTMLInputElement>('input[type="checkbox"]');
		await act(async () => checkbox!.click());
		await act(async () => button("settings.computerUse.approval.allow").click());

		expect(mocks.respond).toHaveBeenCalledWith({
			requestId: "request-1",
			allowed: true,
			allowedBundleIds: ["com.apple.Safari"],
		});
	});

	it("keeps deny available when status fails and does not use another conversation's title", async () => {
		mocks.status.mockRejectedValue(new Error("status unavailable"));
		await render([{
			...session,
			session: { ...session.session, id: "other-session" },
		} as WorkspaceSessionSummary]);
		expect(document.body.textContent).toContain("settings.computerUse.approval.unknownConversation:session-1");
		const deny = button("settings.computerUse.approval.deny");
		expect(deny.disabled).toBe(false);
		await act(async () => deny.click());
		expect(mocks.respond).toHaveBeenCalledWith({
			requestId: "request-1", allowed: false, allowedBundleIds: [],
		});
	});

	it("opens only the missing macOS permission and refreshes the request status", async () => {
		mocks.status.mockResolvedValue({
			...status,
			accessibility: { granted: false, canRequest: true },
		});
		await render();
		await act(async () => button("settings.computerUse.approval.openAccessibility").click());
		expect(mocks.access).toHaveBeenCalledWith("accessibility");
		expect(mocks.access).toHaveBeenCalledTimes(1);
		expect(mocks.respond).not.toHaveBeenCalled();
	});

	it("closes an expired request before it can be allowed", async () => {
		vi.useFakeTimers();
		mocks.list.mockResolvedValue([{ ...request, remainingMs: 1_000 }]);
		await render();

		await act(async () => {
			await vi.advanceTimersByTimeAsync(1_100);
		});

		expect(document.body.textContent).not.toContain(request.reason);
		expect([...document.querySelectorAll("button")]
			.some((element) => element.textContent?.includes("settings.computerUse.approval.allow")))
			.toBe(false);
		expect(mocks.respond).not.toHaveBeenCalled();
	});

	it("notifies once when the backend removes a request after its deadline", async () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-09-14T12:00:00Z"));
		mocks.list.mockResolvedValue([{ ...request, remainingMs: 1_000 }]);
		await render();

		await act(async () => {
			vi.setSystemTime(new Date("2026-09-14T12:00:01.100Z"));
			client.setQueryData(["computer-use", "pending-requests"], []);
			await vi.advanceTimersByTimeAsync(300);
		});

		expect(document.body.textContent).not.toContain(request.reason);
		expect(mocks.toastInfo).toHaveBeenCalledExactlyOnceWith("settings.computerUse.approval.expired");
	});

	it("does not report expiration after an explicit denial removes the request", async () => {
		mocks.list.mockReset().mockResolvedValueOnce([request]).mockResolvedValue([]);
		await render();
		await act(async () => button("settings.computerUse.approval.deny").click());

		expect(mocks.respond).toHaveBeenCalledWith({
			requestId: "request-1", allowed: false, allowedBundleIds: [],
		});
		expect(mocks.toastInfo).not.toHaveBeenCalled();
	});

	it("does not carry an old permission failure or selected app into a replacement request", async () => {
		const replacement = {
			...request,
			requestId: "request-2",
			sessionId: "session-2",
			reason: "Inspect the replacement app",
		};
		mocks.status.mockResolvedValue({
			...status,
			accessibility: { granted: false, canRequest: true },
		});
		let rejectAccess: ((error: Error) => void) | null = null;
		mocks.access.mockImplementation(() => new Promise((_, reject) => {
			rejectAccess = reject;
		}));
		await render();
		await vi.waitFor(() => expect(document.querySelector<HTMLInputElement>('input[type="checkbox"]')).not.toBeNull());
		const checkbox = document.querySelector<HTMLInputElement>('input[type="checkbox"]');
		expect(checkbox).not.toBeNull();
		await act(async () => checkbox!.click());
		expect(checkbox!.checked).toBe(true);
		await act(async () => button("settings.computerUse.approval.openAccessibility").click());

		await act(async () => {
			client.setQueryData(["computer-use", "pending-requests"], [replacement]);
		});
		await vi.waitFor(() => expect(document.body.textContent).toContain(replacement.reason));
		rejectAccess!(new Error("request A permission failure"));
		await act(async () => {
			await Promise.resolve();
		});

		expect(document.body.textContent).not.toContain("request A permission failure");
		const replacementCheckbox = document.querySelector<HTMLInputElement>('input[type="checkbox"]');
		expect(replacementCheckbox?.checked).toBe(false);
	});
});
