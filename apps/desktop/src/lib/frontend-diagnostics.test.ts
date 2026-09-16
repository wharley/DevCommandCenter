import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

beforeEach(() => {
	localStorage.clear();
	vi.resetModules();
});
afterEach(() => vi.restoreAllMocks());

describe("local frontend diagnostics", () => {
	it("keeps the most recent errors across a module reload, with the captured context", async () => {
		let diagnostics = await import("./frontend-diagnostics");
		diagnostics.setFrontendErrorContext({ appVersion: "0.1.73", sessionId: "session-a" });
		for (let i = 0; i < 12; i++) {
			diagnostics.recordFrontendError("react-caught", new Error(`stream ${i}`), {
				componentStack: "\n at AssistantMessage\n at WorkspacePanel",
			});
		}
		diagnostics.setFrontendErrorContext({ sessionId: "session-b" });
		vi.resetModules();
		diagnostics = await import("./frontend-diagnostics");
		const records = diagnostics.readFrontendErrors();
		expect(records).toHaveLength(10);
		expect(records[0].message).toBe("Error: stream 2");
		expect(records[9].context.sessionId).toBe("session-a");
		expect(records[9].stack).toContain("stream 11");
		expect(records[9].componentStack).toContain("AssistantMessage");
		expect(JSON.parse(diagnostics.exportFrontendErrors()).errors).toHaveLength(10);
	});

	it("captures global errors and promise rejections and removes its listeners", async () => {
		const diagnostics = await import("./frontend-diagnostics");
		const remove = diagnostics.installFrontendErrorListeners();
		window.dispatchEvent(new ErrorEvent("error", { error: new Error("animation failed") }));
		const rejection = new Event("unhandledrejection");
		Object.defineProperty(rejection, "reason", { value: new Error("lazy module failed") });
		window.dispatchEvent(rejection);
		remove();
		window.dispatchEvent(new ErrorEvent("error", { message: "after cleanup" }));
		expect(diagnostics.readFrontendErrors().map((entry) => entry.kind)).toEqual([
			"window-error", "unhandled-rejection",
		]);
	});

	it("still exports errors if local storage is full or inaccessible", async () => {
		const diagnostics = await import("./frontend-diagnostics");
		vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("quota"); });
		diagnostics.recordFrontendError("react-caught", new Error("render failed"));
		vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("disabled"); });
		expect(JSON.parse(diagnostics.exportFrontendErrors()).errors[0].message).toBe("Error: render failed");
	});

	it("recovers from corrupt storage without breaking the error handler", async () => {
		const diagnostics = await import("./frontend-diagnostics");
		localStorage.setItem(diagnostics.FRONTEND_DIAGNOSTICS_KEY, "{broken");
		expect(() => diagnostics.recordFrontendError("react-uncaught", "render failure")).not.toThrow();
		expect(diagnostics.readFrontendErrors()).toHaveLength(1);
	});

	it("bounds text and removes credentials from messages, stacks and source locations", async () => {
		const diagnostics = await import("./frontend-diagnostics");
		diagnostics.recordFrontendError("window-error", new Error(
			"postgres://name:private-password@host/db?token=private-token Bearer private-bearer " + "x".repeat(20_000),
		), { scope: "https://host/app.js?api_key=private-key" });
		const report = diagnostics.exportFrontendErrors();
		for (const secret of ["private-password", "private-token", "private-bearer", "private-key"]) {
			expect(report).not.toContain(secret);
		}
		expect(diagnostics.readFrontendErrors()[0].message.length).toBeLessThanOrEqual(2000);
		expect(diagnostics.readFrontendErrors()[0].stack.length).toBeLessThanOrEqual(6000);
	});

	it("does not serialize arbitrary rejection payloads or throw on unusual error getters", async () => {
		const diagnostics = await import("./frontend-diagnostics");
		diagnostics.recordFrontendError("unhandled-rejection", { prompt: "private transcript" });
		expect(diagnostics.exportFrontendErrors()).not.toContain("private transcript");
		const error = new Error("bad getter");
		Object.defineProperty(error, "message", { get() { throw new Error("getter failed"); } });
		expect(() => diagnostics.recordFrontendError("window-error", error)).not.toThrow();
	});
});
