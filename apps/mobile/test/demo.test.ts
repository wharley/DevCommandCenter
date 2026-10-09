import { runScan, triageGroups } from "@dcc/mobile-core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DemoBackend } from "../src/backend/demo";
import { ApiError } from "../src/backend/types";
import { ThreadSync, type ThreadSnapshot } from "../src/screens/threadSync";

describe("demo mode", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("fills every triage group", async () => {
		const demo = new DemoBackend();
		const scan = runScan((path) => demo.request("GET", path));
		await vi.advanceTimersByTimeAsync(1_000);
		const groups = triageGroups((await scan).scan);
		expect(groups.needsYou.map((p) => p.requestId).sort()).toEqual(["demo-p1", "demo-q1"]);
		expect(groups.running.map((s) => s.sessionId)).toEqual(["demo-s2"]);
		expect(groups.recent.length).toBeGreaterThan(0);
		demo.dispose();
	});

	it("streams a turn, asks for permission, and answers 409 the second time", async () => {
		const demo = new DemoBackend();
		let snapshot: ThreadSnapshot | null = null;
		const sync = new ThreadSync(demo, "demo-s4", (next) => (snapshot = next));
		sync.start();
		await vi.advanceTimersByTimeAsync(1_000);
		const before = snapshot!.state.messages.length;

		const sent = demo.request("POST", "/api/v1/sessions/demo-s4/turns", { prompt: "rode os testes" });
		await vi.advanceTimersByTimeAsync(5_000);
		await sent;
		const permission = snapshot!.state.messages.find((m) => m.kind === "permission" && !m.resolved);
		expect(permission).toBeDefined();
		expect(snapshot!.state.messages.length).toBeGreaterThan(before);

		const requestId = permission!.kind === "permission" ? permission!.requestId : "";
		const allow = demo.request("POST", "/api/v1/sessions/demo-s4/respond-permission", { requestId, behavior: "allow" });
		await vi.advanceTimersByTimeAsync(10_000);
		await allow;
		const answer = [...snapshot!.state.messages].reverse().find((m) => m.kind === "assistant");
		expect(answer?.kind === "assistant" && answer.completed && answer.text).toMatch(/42 testes/);

		const again = demo.request("POST", "/api/v1/sessions/demo-s4/respond-permission", { requestId, behavior: "allow" });
		const caught = again.catch((e: unknown) => e);
		await vi.advanceTimersByTimeAsync(500);
		expect(await caught).toBeInstanceOf(ApiError);
		expect(((await caught) as ApiError).status).toBe(409);
		sync.stop();
		demo.dispose();
	});

	it("stops a running turn", async () => {
		const demo = new DemoBackend();
		let snapshot: ThreadSnapshot | null = null;
		const sync = new ThreadSync(demo, "demo-s2", (next) => (snapshot = next));
		sync.start();
		await vi.advanceTimersByTimeAsync(1_000);
		const abort = demo.request("POST", "/api/v1/sessions/demo-s2/abort", {});
		await vi.advanceTimersByTimeAsync(1_000);
		await abort;
		const answer = snapshot!.state.messages.find((m) => m.kind === "assistant");
		expect(answer?.kind === "assistant" && answer.aborted).toBe(true);
		sync.stop();
		demo.dispose();
	});
});
