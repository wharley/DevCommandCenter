import { afterEach, expect, it, vi } from "vitest";
import {
	createMobileTask,
	newRequestId,
	pendingTask,
	type TaskInput,
} from "./task-api";
import { clearLocalData } from "./local-data";
const session = {
	backendUrl: "http://host",
	deviceId: "one",
	sessionToken: "token",
	createdAt: "now",
};
const task: TaskInput = {
	requestId: "once",
	workspaceId: "workspace",
	repositoryId: null,
	title: "Title",
	prompt: "Prompt",
	providerId: "codex",
	model: null,
	planMode: false,
};
afterEach(() => {
	vi.unstubAllGlobals();
	clearLocalData();
});
it("retains the original request after a network failure and reuses it on retry", async () => {
	const fetch = vi
		.fn()
		.mockRejectedValueOnce(new TypeError("offline"))
		.mockResolvedValueOnce(
			new Response(
				JSON.stringify({ state: "started", sessionId: "created-once" }),
				{ headers: { "Content-Type": "application/json" } },
			),
		);
	vi.stubGlobal("fetch", fetch);
	await expect(createMobileTask(session, task)).rejects.toThrow();
	expect(pendingTask(session)).toEqual(task);
	expect(pendingTask({ ...session, deviceId: "other" })).toBeNull();
	expect(await createMobileTask(session, pendingTask(session)!)).toMatchObject({
		sessionId: "created-once",
	});
	expect(fetch.mock.calls[0]![1].body).toBe(fetch.mock.calls[1]![1].body);
});
it("generates UUIDs without requiring the HTTPS-only randomUUID API", () => {
	vi.stubGlobal("crypto", {
		getRandomValues: (bytes: Uint8Array) => bytes.fill(123),
	});
	expect(newRequestId()).toMatch(
		/^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/,
	);
});
