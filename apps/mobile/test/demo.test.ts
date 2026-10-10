import { projectGroups, runScan, taskRequestBody, triageGroups, type MobileCatalog, type TaskProgress } from "@dcc/mobile-core";
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

	it("lists a project with no workspace and creates a task in it", async () => {
		const demo = new DemoBackend();
		const read = <T,>(method: string, path: string, body?: unknown) => {
			const reply = demo.request<T>(method, path, body);
			reply.catch(() => {}); // Observed below; keeps the fake timers from reporting it unhandled.
			return vi.advanceTimersByTimeAsync(500).then(() => reply);
		};
		const catalog = await read<MobileCatalog>("GET", "/api/v1/mobile/catalog");
		const empty = projectGroups(catalog.repositories, (await read("GET", "/api/v1/combs")) as never, []).find((g) => g.combs.length === 0);
		expect(empty?.name).toBe("app-mobile");

		const body = taskRequestBody({
			requestId: "00000000-0000-4000-8000-000000000001",
			repositoryId: empty!.repositoryId!,
			prompt: "Crie a tela de login",
			providerId: "codex",
			model: "gpt-5.5-codex",
			worktree: false,
		});
		expect((await read<TaskProgress>("POST", "/api/v1/mobile/tasks", body)).state).toBe("creating");
		const done = await read<TaskProgress>("GET", `/api/v1/mobile/tasks/${body.requestId}`);
		expect(done).toMatchObject({ state: "started" });
		// Retrying the same creation answers the same outcome; a changed draft is refused.
		expect(await read<TaskProgress>("POST", "/api/v1/mobile/tasks", body)).toEqual(done);
		await expect(read("POST", "/api/v1/mobile/tasks", { ...body, prompt: "outra" })).rejects.toBeInstanceOf(ApiError);

		const sessions = await read<Array<{ sessionId: string; providerId: string; workspaceBranch: string }>>("GET", "/api/v1/sessions/search");
		expect(sessions[0]).toMatchObject({ sessionId: done.sessionId, providerId: "codex", workspaceBranch: "main" });
		const after = projectGroups(catalog.repositories, (await read("GET", "/api/v1/combs")) as never, []);
		expect(after.find((g) => g.name === "app-mobile")?.combs).toHaveLength(1);
		demo.dispose();
	});
});
