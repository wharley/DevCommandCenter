import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock, openExternalMock } = vi.hoisted(() => ({
	invokeMock: vi.fn(),
	openExternalMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: invokeMock,
}));
vi.mock("./shell-api", () => ({
	openExternal: openExternalMock,
}));

import {
	armComputerUse,
	disarmComputerUse,
	getComputerUseStatus,
	listComputerUsePendingRequests,
	requestComputerUseAccess,
	respondComputerUseControlRequest,
} from "./computer-use-api";

describe("Computer Use API", () => {
	beforeEach(() => {
		invokeMock.mockReset();
		openExternalMock.mockReset().mockResolvedValue({ success: true });
		Object.defineProperty(window, "__TAURI_INTERNALS__", {
			configurable: true,
			value: {},
		});
	});

	it("loads status with the optional current session", async () => {
		await getComputerUseStatus("session-1");

		expect(invokeMock).toHaveBeenCalledWith("computer_use_status", {
			input: { sessionId: "session-1", includeTargets: true },
		});
	});

	it("arms only the apps explicitly selected by the user", async () => {
		await armComputerUse({
			sessionId: "session-1",
			allowedBundleIds: ["com.apple.Safari"],
		});

		expect(invokeMock).toHaveBeenCalledWith("computer_use_arm", {
			input: {
				sessionId: "session-1",
				allowedBundleIds: ["com.apple.Safari"],
			},
		});
	});

	it("supports immediate revocation and independent OS permission requests", async () => {
		await disarmComputerUse({ sessionId: "session-1" });
		await requestComputerUseAccess("screenRecording");

		expect(invokeMock).toHaveBeenNthCalledWith(1, "computer_use_disarm", {
			input: { sessionId: "session-1" },
		});
		expect(invokeMock).toHaveBeenNthCalledWith(2, "computer_use_request_access", {
			input: { kind: "screenRecording" },
		});
	});

	it("opens the matching macOS Privacy panel before requesting permission", async () => {
		await requestComputerUseAccess("accessibility");

		expect(openExternalMock).toHaveBeenCalledWith(
			"x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
		);
		expect(invokeMock).toHaveBeenCalledWith("computer_use_request_access", {
			input: { kind: "accessibility" },
		});
	});

	it("does not send the native permission request when Privacy settings fail to open", async () => {
		openExternalMock.mockRejectedValue(new Error("open failed"));

		await expect(requestComputerUseAccess("screenRecording")).rejects.toThrow(
			"open failed",
		);
		expect(invokeMock).not.toHaveBeenCalled();
	});

	it("stops when the shell reports that opening Privacy settings was unsuccessful", async () => {
		openExternalMock.mockResolvedValue({ success: false });

		await expect(requestComputerUseAccess("screenRecording")).rejects.toThrow(
			"Could not open macOS Privacy settings",
		);
		expect(invokeMock).not.toHaveBeenCalled();
	});

	it("does not persist or invoke from the browser runtime", async () => {
		delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;

		const status = await getComputerUseStatus();
		expect(status.supported).toBe(false);
		await expect(disarmComputerUse({ sessionId: "session-1" })).rejects.toThrow(
			"desktop runtime",
		);
		expect(invokeMock).not.toHaveBeenCalled();
	});

	it("lists pending requests and responds with the exact selected app scope", async () => {
		invokeMock
			.mockResolvedValueOnce({ requests: [{
				requestId: "request-1", sessionId: "session-1", workspaceId: "workspace-1",
				providerId: "codex", reason: "Open the selected app", remainingMs: 180000,
			}] })
			.mockResolvedValueOnce({ grant: { sessionId: "session-1", armed: true, remainingMs: 600000, allowedBundleIds: ["com.apple.Safari"] } });

		await expect(listComputerUsePendingRequests()).resolves.toEqual([expect.objectContaining({ requestId: "request-1" })]);
		await respondComputerUseControlRequest({ requestId: "request-1", allowed: true, allowedBundleIds: ["com.apple.Safari"] });

		expect(invokeMock).toHaveBeenNthCalledWith(1, "computer_use_list_pending_requests");
		expect(invokeMock).toHaveBeenNthCalledWith(2, "computer_use_respond_control_request", {
			input: { requestId: "request-1", allowed: true, allowedBundleIds: ["com.apple.Safari"] },
		});
	});

	it("returns no pending requests in the browser runtime", async () => {
		delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
		await expect(listComputerUsePendingRequests()).resolves.toEqual([]);
	});
});
