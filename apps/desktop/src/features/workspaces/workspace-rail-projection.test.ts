import { describe, expect, it } from "vitest";
import type { Repository } from "@dcc/contracts";
import {
	freezeRailGroupOrder,
	projectWorkspaceRailGroups,
	projectWorkspaceRepositories,
	splitIdleRailGroups,
	workspaceRailGroupSignal,
} from "./workspace-rail-projection";

describe("projectWorkspaceRailGroups", () => {
	it("groups active workspaces by project path and separates waiting and completed rows", () => {
		const { activeGroups, waitingRows, completedRows } = projectWorkspaceRailGroups(
			[
				{
					id: "a",
					name: "Alpha",
					branch: "main",
					status: "ready",
					rootPath: "/projects/alpha",
					updatedAt: "2026-04-10T10:00:00.000Z",
				},
				{
					id: "b",
					name: "Alpha hotfix",
					branch: "hotfix",
					status: "ready",
					rootPath: "/projects/alpha",
					updatedAt: "2026-04-11T10:00:00.000Z",
				},
				{
					id: "c",
					name: "Waiting Spike",
					branch: "spike",
					status: "archived",
					projectId: "project-spike",
				},
				{
					id: "d",
					name: "Completed Feature",
					branch: "feature/done",
					status: "completed",
					projectId: "project-done",
				},
			],
			[
				{
					id: "/projects/alpha",
					projectId: "alpha",
					name: "alpha",
					displayName: "Customer Portal",
					icon: "rocket",
					color: "violet",
					pinnedAt: null,
					rootPath: "/projects/alpha",
					baseBranch: "main",
					remote: null,
					remoteUrl: null,
					forgeProvider: null,
					forgeLogin: null,
					createdAt: "2026-04-10T10:00:00.000Z",
					updatedAt: "2026-04-11T10:00:00.000Z",
				},
			],
		);

		expect(activeGroups).toHaveLength(1);
		expect(activeGroups[0]).toMatchObject({
			label: "Customer Portal",
			rows: [
				{ id: "b", name: "Alpha hotfix" },
				{ id: "a", name: "Alpha" },
			],
		});
		expect(waitingRows).toEqual([
			{
				id: "c",
				name: "Waiting Spike",
				branch: "spike",
				status: "archived",
				projectId: "project-spike",
			},
		]);
		expect(completedRows).toEqual([
			{
				id: "d",
				name: "Completed Feature",
				branch: "feature/done",
				status: "completed",
				projectId: "project-done",
			},
		]);
	});

	it("keeps repository groups visible even when they have no active workspaces", () => {
		const { activeGroups, waitingRows, completedRows } = projectWorkspaceRailGroups(
			[],
			[
				{
					id: "/projects/alpha",
					projectId: "alpha",
					name: "alpha",
					displayName: null,
					icon: null,
					color: null,
					pinnedAt: null,
					rootPath: "/projects/alpha",
					baseBranch: "main",
					remote: null,
					remoteUrl: null,
					forgeProvider: null,
					forgeLogin: null,
					createdAt: "2026-04-10T10:00:00.000Z",
					updatedAt: "2026-04-11T10:00:00.000Z",
				},
			],
		);

		expect(activeGroups).toEqual([
			{
				id: expect.any(String),
				label: "alpha",
				sourceKey: "/projects/alpha",
				pinnedAt: null,
				rows: [],
			},
		]);
		expect(waitingRows).toEqual([]);
		expect(completedRows).toEqual([]);
	});

	it("promotes pinned projects and pinned tasks without changing the remaining order", () => {
		const { activeGroups } = projectWorkspaceRailGroups(
			[
				{
					id: "newer",
					name: "Newer task",
					branch: "main",
					status: "ready",
					rootPath: "/projects/zeta",
					pinnedAt: null,
					updatedAt: "2026-04-12T10:00:00.000Z",
				},
				{
					id: "pinned",
					name: "Pinned task",
					branch: "main",
					status: "ready",
					rootPath: "/projects/zeta",
					pinnedAt: "2026-04-10T10:00:00.000Z",
					updatedAt: "2026-04-10T10:00:00.000Z",
				},
			],
			[
				{
					id: "/projects/alpha",
					projectId: "alpha",
					name: "alpha",
					displayName: null,
					icon: null,
					color: null,
					pinnedAt: null,
					rootPath: "/projects/alpha",
					baseBranch: "main",
					remote: null,
					remoteUrl: null,
					forgeProvider: null,
					forgeLogin: null,
					createdAt: "2026-04-11T10:00:00.000Z",
					updatedAt: "2026-04-11T10:00:00.000Z",
				},
				{
					id: "/projects/zeta",
					projectId: "zeta",
					name: "zeta",
					displayName: null,
					icon: null,
					color: null,
					pinnedAt: "2026-04-10T10:00:00.000Z",
					rootPath: "/projects/zeta",
					baseBranch: "main",
					remote: null,
					remoteUrl: null,
					forgeProvider: null,
					forgeLogin: null,
					createdAt: "2026-04-10T10:00:00.000Z",
					updatedAt: "2026-04-10T10:00:00.000Z",
				},
			],
		);

		expect(activeGroups.map((group) => group.label)).toEqual(["zeta", "alpha"]);
		expect(activeGroups[0]?.rows.map((workspace) => workspace.id)).toEqual([
			"pinned",
			"newer",
		]);
	});

	it("keeps pinned projects in pin order and the rest by the user's last interaction", () => {
		const { activeGroups } = projectWorkspaceRailGroups(
			[
				{ id: "a1", name: "A", branch: "main", status: "ready", rootPath: "/projects/alpha", createdAt: "2026-04-01T10:00:00.000Z" },
				{ id: "b1", name: "B", branch: "main", status: "ready", rootPath: "/projects/beta", createdAt: "2026-04-02T10:00:00.000Z", updatedAt: "2026-04-20T10:00:00.000Z" },
				{ id: "g1", name: "G", branch: "main", status: "ready", rootPath: "/projects/gamma", createdAt: "2026-04-03T10:00:00.000Z" },
				{ id: "p1", name: "P", branch: "main", status: "ready", rootPath: "/projects/pinned-late", createdAt: "2026-04-01T10:00:00.000Z" },
			],
			[
				railRepository("empty", null),
				railRepository("alpha", null),
				railRepository("beta", null),
				railRepository("gamma", null),
				railRepository("pinned-late", "2026-04-09T10:00:00.000Z"),
				railRepository("pinned-early", "2026-04-05T10:00:00.000Z"),
			],
			// Alpha was touched last; beta's newer metadata update must not count.
			{ a1: "2026-04-15T10:00:00.000Z", p1: "2026-04-30T10:00:00.000Z" },
		);

		expect(activeGroups.map((group) => group.label)).toEqual([
			"pinned-early",
			"pinned-late",
			"alpha",
			"gamma",
			"beta",
			"empty",
		]);
	});

	it("freezes unpinned project order while keeping pins live and new projects on top", () => {
		const group = (id: string, pinnedAt: string | null = null) => ({
			id,
			label: id,
			sourceKey: id,
			pinnedAt,
			rows: [],
		});

		const frozen = freezeRailGroupOrder(
			[group("pin", "2026-04-01T10:00:00.000Z"), group("new"), group("c"), group("a"), group("b")],
			["a", "b", "c", "pin"],
		);

		expect(frozen.map((entry) => entry.id)).toEqual(["pin", "new", "a", "b", "c"]);
	});

	it("tucks recent projects without tasks away, keeping pins and what is on screen", () => {
		const task = {
			id: "t",
			name: "t",
			branch: "main",
			status: "ready" as const,
			rootPath: "/projects/busy",
		};
		const group = (id: string, pinnedAt: string | null, rows: (typeof task)[] = []) => ({
			id,
			label: id,
			sourceKey: id,
			pinnedAt,
			rows,
		});
		const groups = [
			group("pinned-empty", "2026-04-01T10:00:00.000Z"),
			group("busy", null, [task]),
			group("just-emptied", null),
			group("idle", null),
		];

		const { visibleGroups, idleGroups } = splitIdleRailGroups(groups, new Set(["just-emptied"]));

		expect(visibleGroups.map((entry) => entry.id)).toEqual(["pinned-empty", "busy", "just-emptied"]);
		expect(idleGroups.map((entry) => entry.id)).toEqual(["idle"]);
		expect(splitIdleRailGroups(groups).idleGroups.map((entry) => entry.id)).toEqual([
			"just-emptied",
			"idle",
		]);
	});

	it("builds repository-level options for quick workspace creation", () => {
		const repositories = projectWorkspaceRepositories([
			{
				id: "a",
				name: "Alpha",
				branch: "main",
				status: "ready",
				projectId: "alpha",
				rootPath: "/projects/alpha",
				updatedAt: "2026-04-10T10:00:00.000Z",
			},
			{
				id: "b",
				name: "Alpha hotfix",
				branch: "hotfix",
				status: "archived",
				projectId: "alpha",
				rootPath: "/projects/alpha",
				updatedAt: "2026-04-11T10:00:00.000Z",
			},
			{
				id: "c",
				name: "Beta",
				branch: "develop",
				status: "ready",
				projectId: "beta",
				rootPath: "/projects/beta",
				updatedAt: "2026-04-12T10:00:00.000Z",
			},
			{
				id: "d",
				name: "Loose Workspace",
				branch: "spike",
				status: "ready",
			},
		]);

		expect(repositories).toEqual([
			{
				sourceKey: "/projects/beta",
				label: "beta",
				projectId: "beta",
				workspaceRoot: "/projects/beta",
				branch: "develop",
				updatedAt: "2026-04-12T10:00:00.000Z",
			},
			{
				sourceKey: "/projects/alpha",
				label: "alpha",
				projectId: "alpha",
				workspaceRoot: "/projects/alpha",
				branch: "hotfix",
				updatedAt: "2026-04-11T10:00:00.000Z",
			},
		]);
	});

	it("falls back to workspace identity when path and project id are missing", () => {
		const { activeGroups } = projectWorkspaceRailGroups([
			{
				id: "z",
				name: "Loose Workspace",
				branch: "feat/loose",
				status: "ready",
			},
		]);

		expect(activeGroups).toHaveLength(1);
		expect(activeGroups[0]).toMatchObject({
			label: "Loose Workspace",
			rows: [{ id: "z", name: "Loose Workspace" }],
		});
	});
});

describe("task order inside a project", () => {
	const task = (id: string, updatedAt: string, pinnedAt?: string) => ({
		id,
		name: id,
		branch: "main",
		status: "ready" as const,
		rootPath: "/projects/alpha",
		updatedAt,
		pinnedAt: pinnedAt ?? null,
	});

	it("moves the task the user last worked on to the top", () => {
		const { activeGroups } = projectWorkspaceRailGroups(
			[
				task("older", "2026-09-30T10:00:00.000Z"),
				task("newer", "2026-09-30T11:00:00.000Z"),
				task("sixth", "2026-09-30T09:00:00.000Z"),
			],
			[],
			{ sixth: "2026-09-30T12:00:00.000Z" },
		);

		expect(activeGroups[0]!.rows.map((row) => row.id)).toEqual([
			"sixth",
			"newer",
			"older",
		]);
	});

	it("keeps pinned tasks above recent activity", () => {
		const { activeGroups } = projectWorkspaceRailGroups(
			[
				task("pinned", "2026-09-30T08:00:00.000Z", "2026-09-01T00:00:00.000Z"),
				task("busy", "2026-09-30T09:00:00.000Z"),
			],
			[],
			{ busy: "2026-09-30T12:00:00.000Z" },
		);

		expect(activeGroups[0]!.rows.map((row) => row.id)).toEqual(["pinned", "busy"]);
	});
});

describe("workspaceRailGroupSignal", () => {
	const row = (id: string, status: "ready" | "setup_pending" = "ready") => ({
		id,
		name: id,
		branch: "main",
		status,
	});
	const running = {
		state: "active" as const,
		startedAt: "2026-09-30T10:00:00.000Z",
		completedAt: null,
	};

	it("stays quiet when nothing in the project is running or blocked", () => {
		expect(
			workspaceRailGroupSignal([row("a")], {
				a: { state: "completed", startedAt: null, completedAt: null },
			}),
		).toBeNull();
	});

	it("reports a running agent", () => {
		expect(workspaceRailGroupSignal([row("a"), row("b")], { b: running })).toBe(
			"running",
		);
	});

	it("treats an agent waiting on the user as needing attention", () => {
		expect(
			workspaceRailGroupSignal([row("a"), row("b")], {
				a: running,
				b: {
					state: "waiting",
					startedAt: null,
					completedAt: null,
					waitingFor: "input",
				},
			}),
		).toBe("attention");
	});

	it("lets something that needs the user win over a running agent", () => {
		expect(
			workspaceRailGroupSignal([row("a"), row("b", "setup_pending")], {
				a: running,
			}),
		).toBe("attention");
	});
});

describe("snoozed tasks in the rail", () => {
	const now = Date.parse("2026-10-07T12:00:00Z");
	const task = (id: string, snoozedUntil: string | null) => ({
		id,
		name: id,
		branch: "main",
		status: "ready" as const,
		rootPath: "/repo",
		snoozedUntil,
	});

	it("keeps a snoozed task on hold until its time, then back in its project", () => {
		const { activeGroups, waitingRows } = projectWorkspaceRailGroups(
			[task("later", "2026-10-07T13:00:00Z"), task("woken", "2026-10-07T11:00:00Z"), task("plain", null)],
			[],
			{},
			now,
		);
		expect(waitingRows.map((row) => row.id)).toEqual(["later"]);
		expect(activeGroups.flatMap((group) => group.rows.map((row) => row.id)).sort()).toEqual([
			"plain",
			"woken",
		]);
	});
});

function railRepository(name: string, pinnedAt: string | null): Repository {
	return {
		id: `/projects/${name}`,
		projectId: name,
		name,
		displayName: null,
		icon: null,
		color: null,
		pinnedAt,
		rootPath: `/projects/${name}`,
		baseBranch: "main",
		remote: null,
		remoteUrl: null,
		forgeProvider: null,
		forgeLogin: null,
		createdAt: "2026-04-01T10:00:00.000Z",
		updatedAt: "2026-04-01T10:00:00.000Z",
	};
}
