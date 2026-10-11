import { describe, expect, it } from "vitest";
import {
	defaultModelId,
	defaultProviderId,
	projectGroups,
	providerUnavailableReason,
	accessLabel,
	accessPolicies,
	pickAccessPolicy,
	taskRequestBody,
	taskTitle,
	type CatalogProvider,
	type CatalogRepository,
} from "./catalog.ts";
import type { Comb, SessionSearchResult } from "./triage.ts";

const provider = (id: string, extra: Partial<CatalogProvider> = {}): CatalogProvider => ({
	id,
	label: id,
	models: [],
	health: "Healthy",
	stable: true,
	...extra,
});

const repo = (id: string, name: string, rootPath: string): CatalogRepository => ({
	id,
	projectId: `p-${id}`,
	name,
	displayName: null,
	rootPath,
	baseBranch: "main",
});

const comb = (id: string, projectPath: string, extra: Partial<Comb> = {}): Comb => ({
	id,
	name: id,
	branch: `feat/${id}`,
	projectId: null,
	projectName: null,
	projectPath,
	worktreePath: null,
	status: "ready",
	lastOpenedAt: null,
	...extra,
});

describe("catalog", () => {
	it("picks providers like the desktop: remembered, then stable, skipping unusable ones", () => {
		const providers = [
			provider("cursor", { stable: false }),
			provider("claude", { health: { Unhealthy: { reason: "Faça login" } } }),
			provider("codex"),
			provider("gemini", { enabled: false }),
		];
		expect(providerUnavailableReason(providers[1]!)).toBe("Faça login");
		expect(providerUnavailableReason(providers[3]!)).toBe("Desligado no computador");
		expect(providerUnavailableReason(provider("x", { health: { Degraded: { reason: "lento" } } }))).toBeNull();
		expect(defaultProviderId(providers)).toBe("codex");
		expect(defaultProviderId(providers, "cursor")).toBe("cursor");
		expect(defaultProviderId(providers, "claude")).toBe("codex");
		expect(defaultProviderId([])).toBeNull();
	});

	it("defaults to the recommended model", () => {
		expect(defaultModelId(provider("c", { models: [{ id: "a", label: "A" }, { id: "b", label: "B", recommended: true }] }))).toBe("b");
		expect(defaultModelId(provider("c", { models: [{ id: "a", label: "A" }] }))).toBe("a");
		expect(defaultModelId(provider("c"))).toBeNull();
	});

	it("titles the task from the first line and maps the worktree checkbox", () => {
		expect(taskTitle("\n  Corrija   o login \nmais detalhes")).toBe("Corrija o login");
		expect(taskTitle("x".repeat(80))).toHaveLength(60);
		expect(taskTitle("   ")).toBe("Nova tarefa");
		const base = { requestId: "r", repositoryId: "repo", prompt: " faça \n", providerId: "claude", model: null };
		expect(taskRequestBody({ ...base, worktree: true })).toMatchObject({
			repositoryId: "repo",
			prompt: "faça",
			title: "faça",
			isolationMode: "protectedWorktree",
		});
		expect(taskRequestBody({ ...base, worktree: false }).isolationMode).toBe("localDirect");
		expect(taskRequestBody({ ...base, worktree: true, access: "full_access" })).toMatchObject({
			approvalPolicy: "full_access",
		});
		expect(taskRequestBody({ ...base, worktree: true, access: null })).not.toHaveProperty("approvalPolicy");
	});

	it("offers only the access levels the agent enforces, picked like the desktop", () => {
		const all = provider("codex", { capabilities: { approvalPolicies: ["read_only", "ask", "auto", "full_access"] } });
		expect(accessPolicies(all)).toEqual(["ask", "auto", "full_access"]);
		expect(accessPolicies(provider("cursor"))).toEqual([]);
		const askOnly = provider("x", { capabilities: { approvalPolicies: ["ask"] } });
		expect(pickAccessPolicy(accessPolicies(all))).toBe("auto");
		expect(pickAccessPolicy(accessPolicies(all), null, "full_access")).toBe("full_access");
		expect(pickAccessPolicy(accessPolicies(askOnly), "full_access")).toBe("ask");
		expect(pickAccessPolicy([])).toBeNull();
		expect(accessLabel("full_access")).toBe("Acesso completo");
	});

	it("lists every project, including ones without a workspace yet", () => {
		const repositories = [repo("1", "zeta", "/z"), repo("2", "alfa", "/a")];
		const combs = [
			comb("w1", "/z"),
			comb("w2", "/z", { status: "archived" }),
			comb("w3", "/elsewhere", { projectName: "solto" }),
		];
		const groups = projectGroups(repositories, combs, []);
		expect(groups.map((g) => [g.name, g.combs.map((c) => c.id)])).toEqual([
			["solto", ["w3"]],
			["zeta", ["w1"]],
			["alfa", []],
		]);
		expect(groups[2]!.repositoryId).toBe("2");
		expect(groups[0]!.repositoryId).toBeNull();
	});

	it("floats workspaces with sessions and searches by project or workspace", () => {
		const repositories = [repo("1", "loja", "/l")];
		const combs = [comb("a", "/l", { lastOpenedAt: "2026-10-10" }), comb("b", "/l", { lastOpenedAt: "2026-10-01" })];
		const sessions = [{ sessionId: "s", workspaceId: "b", updatedAt: "2026-10-10" } as SessionSearchResult];
		expect(projectGroups(repositories, combs, sessions)[0]!.combs.map((c) => c.id)).toEqual(["b", "a"]);
		expect(projectGroups(repositories, combs, sessions, "LOJA")[0]!.combs).toHaveLength(2);
		expect(projectGroups(repositories, combs, sessions, "feat/a")[0]!.combs.map((c) => c.id)).toEqual(["a"]);
		expect(projectGroups(repositories, combs, sessions, "nada")).toEqual([]);
	});
});
