import { describe, expect, it, vi } from "vitest";
import { composerTurnFromRaw } from "@/features/composer/composer-turn";
import { newQuickLaunch, type QuickLaunch } from "./api";
import { executeQuickLaunch, type LaunchDependencies } from "./launch";

function fixture(mode: "localDirect" | "protectedWorktree" = "localDirect") {
	const initial = newQuickLaunch({
		projectId: "project",
		rootPath: "/repo",
		baseBranch: "main",
		isolationMode: mode,
		providerId: "codex",
		modelId: "model",
		providerRuntime: null,
		turn: composerTurnFromRaw("Fix the bug @/tmp/screenshot.png"),
	});
	let durable: QuickLaunch | null = null;
	const deps: LaunchDependencies = {
		begin: vi.fn(async (launch: QuickLaunch): Promise<QuickLaunch> => {
			if (durable) return durable;
			durable = launch;
			return launch;
		}),
		checkpoint: vi.fn(async (launch: QuickLaunch): Promise<QuickLaunch> => {
			if (
				!durable ||
				durable.id !== launch.id ||
				launch.revision !== durable.revision + 1
			)
				throw new Error("Stale launch");
			durable = structuredClone(launch);
			return durable;
		}),
		createWorkspace: vi.fn(async (launch) => {
			expect(durable?.phase).toBe("creating");
			expect(launch.request.isolationMode).toBe(mode);
			return "workspace";
		}),
		startSession: vi.fn(async (launch) => {
			expect(durable?.phase).toBe("starting");
			expect(launch.workspaceId).toBe("workspace");
			return "session";
		}),
		send: vi.fn(async (launch) => {
			expect(durable?.phase).toBe("sending");
			expect(launch.sessionId).toBe("session");
		}),
		onProgress: vi.fn(),
	};
	return { initial, deps, durable: () => durable };
}
describe("quick launch delivery", () => {
	it.each(["localDirect", "protectedWorktree"] as const)(
		"starts %s with durable checkpoints and the original prompt",
		async (mode) => {
			const { initial, deps, durable } = fixture(mode);
			expect((await executeQuickLaunch(initial, deps)).phase).toBe("completed");
			expect(durable()?.request.turn.rawPrompt).toBe(
				initial.request.turn.rawPrompt,
			);
			await executeQuickLaunch(initial, deps);
			expect(deps.createWorkspace).toHaveBeenCalledTimes(1);
			expect(deps.send).toHaveBeenCalledTimes(1);
		},
	);
	it("keeps the created task when session startup fails", async () => {
		const { initial, deps, durable } = fixture();
		deps.startSession = vi.fn(async () => {
			throw new Error("Provider unavailable");
		});
		const result = await executeQuickLaunch(initial, deps);
		expect(result).toMatchObject({
			phase: "failed",
			workspaceId: "workspace",
			sessionId: null,
			error: "Provider unavailable",
		});
		expect(durable()?.request).toEqual(initial.request);
		await executeQuickLaunch(initial, deps);
		expect(deps.createWorkspace).toHaveBeenCalledTimes(1);
		expect(deps.send).not.toHaveBeenCalled();
	});
	it("never repeats a send with an uncertain response", async () => {
		const { initial, deps } = fixture();
		deps.send = vi.fn(async () => {
			throw new Error("Response lost after send");
		});
		expect(await executeQuickLaunch(initial, deps)).toMatchObject({
			phase: "failed",
			workspaceId: "workspace",
			sessionId: "session",
		});
		await executeQuickLaunch(initial, deps);
		expect(deps.send).toHaveBeenCalledTimes(1);
	});
	it("does not create anything if the claim cannot be persisted", async () => {
		const { initial, deps } = fixture();
		deps.checkpoint = vi.fn(async () => {
			throw new Error("Disk full");
		});
		await expect(executeQuickLaunch(initial, deps)).rejects.toThrow(
			"Disk full",
		);
		expect(deps.createWorkspace).not.toHaveBeenCalled();
	});
	it("allows only one racing submission to claim the request", async () => {
		const { initial, deps } = fixture();
		await Promise.allSettled([
			executeQuickLaunch(initial, deps),
			executeQuickLaunch(initial, deps),
		]);
		expect(deps.createWorkspace).toHaveBeenCalledTimes(1);
		expect(deps.send).toHaveBeenCalledTimes(1);
	});
	it("retains the workspace identity when its checkpoint fails", async () => {
		const { initial, deps } = fixture();
		const save = deps.checkpoint;
		deps.checkpoint = async (launch) => {
			if (launch.phase === "workspaceReady") throw new Error("Disk full");
			return save(launch);
		};
		expect(await executeQuickLaunch(initial, deps)).toMatchObject({
			phase: "failed",
			workspaceId: "workspace",
		});
		expect(deps.startSession).not.toHaveBeenCalled();
	});
});
