import { describe, expect, it } from "vitest";
import type {
	WorkspaceGitStatusOutput,
	WorkspacePrStatusOutput,
} from "@dcc/contracts";
import en from "@/i18n/locales/en/common.json";
import ptBR from "@/i18n/locales/pt-BR/common.json";
import {
	buildWorkspaceRailMeta,
	buildWorkspaceRailRecap,
	isMergedPullRequestForBranch,
	railNextActionKey,
} from "./use-workspace-rail-recap";

const cleanGitStatus: WorkspaceGitStatusOutput = {
	staged: [],
	stagedFingerprint: "",
	unstaged: [],
	currentBranch: "feature/sidebar-recap",
	aheadOfRemoteCount: 0,
	behindOfRemoteCount: 0,
	conflictCount: 0,
	mergeInProgress: false,
};

const noPr: WorkspacePrStatusOutput = {
	provider: "github",
	host: "github.com",
	number: null,
	title: null,
	url: null,
	headBranch: null,
	baseBranch: null,
	state: null,
	isDraft: false,
	mergeable: null,
	mergeStateStatus: null,
};

describe("buildWorkspaceRailRecap", () => {
	it("hides the redundant clean message for an untouched workspace", () => {
		expect(
			buildWorkspaceRailRecap({
				branch: "feature/sidebar-recap",
				activity: null,
				gitStatus: cleanGitStatus,
				prStatus: noPr,
			}),
		).toBeNull();
	});

	it("reuses the Inspector working message while a turn is active", () => {
		const result = buildWorkspaceRailRecap({
			branch: "feature/sidebar-recap",
			activity: {
				state: "active",
				startedAt: "2026-07-24T12:00:00.000Z",
				completedAt: null,
			},
			gitStatus: {
				...cleanGitStatus,
				unstaged: [
					{
						path: "src/sidebar.tsx",
						name: "sidebar.tsx",
						absolutePath: "/repo/src/sidebar.tsx",
						status: "M",
						insertions: 12,
						deletions: 3,
					},
				],
			},
			prStatus: noPr,
		});

		expect(result?.recap.messageKey).toBe("working");
		expect(result?.recap.params).toEqual({
			count: 1,
			additions: 12,
			deletions: 3,
		});
	});

	it("surfaces the PR reference and preserves its title for the row tooltip", () => {
		const result = buildWorkspaceRailRecap({
			branch: "feature/sidebar-recap",
			activity: {
				state: "completed",
				startedAt: "2026-07-24T12:00:00.000Z",
				completedAt: "2026-07-24T12:03:00.000Z",
			},
			gitStatus: cleanGitStatus,
			prStatus: {
				...noPr,
				number: 677,
				title: "Show delivery context in workspace rows",
				url: "https://github.com/example/repo/pull/677",
				headBranch: "feature/sidebar-recap",
				baseBranch: "main",
				state: "open",
				mergeable: "MERGEABLE",
				mergeStateStatus: "CLEAN",
			},
		});

		expect(result?.recap.messageKey).toBe("mergeReady");
		expect(result?.recap.params.pr).toBe("PR #677");
		expect(result?.prTitle).toBe("Show delivery context in workspace rows");
	});

	it("reports committed work that is ready for a PR", () => {
		const result = buildWorkspaceRailRecap({
			branch: "feature/sidebar-recap",
			activity: {
				state: "completed",
				startedAt: "2026-07-24T12:00:00.000Z",
				completedAt: "2026-07-24T12:03:00.000Z",
			},
			gitStatus: cleanGitStatus,
			prStatus: noPr,
			committedVsBaseCount: 2,
		});

		expect(result?.recap.messageKey).toBe("readyForPr");
		expect(result?.recap.params.count).toBe(2);
	});
});

describe("isMergedPullRequestForBranch", () => {
	it("rejects an old merged PR discovered by the base commit of a new task", () => {
		expect(
			isMergedPullRequestForBranch(
				{
					...noPr,
					number: 676,
					headBranch: "feature/already-delivered",
					baseBranch: "main",
					state: "merged",
				},
				"main",
			),
		).toBe(false);
	});

	it("accepts a merged PR only when its head branch is the workspace branch", () => {
		expect(
			isMergedPullRequestForBranch(
				{
					...noPr,
					number: 677,
					headBranch: "dcc/fix/task-completion",
					baseBranch: "main",
					state: "MERGED",
				},
				"dcc/fix/task-completion",
			),
		).toBe(true);
	});

	it("does not infer ownership when the forge omits the PR head branch", () => {
		expect(
			isMergedPullRequestForBranch(
				{
					...noPr,
					number: 678,
					state: "merged",
				},
				"dcc/fix/task-completion",
			),
		).toBe(false);
	});
});

describe("buildWorkspaceRailMeta", () => {
	const dirty: WorkspaceGitStatusOutput = {
		...cleanGitStatus,
		staged: [
			{
				path: "a.ts",
				name: "a.ts",
				absolutePath: "/repo/a.ts",
				status: "M",
				insertions: 30,
				deletions: 2,
			},
		],
		unstaged: [
			{
				path: "b.ts",
				name: "b.ts",
				absolutePath: "/repo/b.ts",
				status: "M",
				insertions: 10,
				deletions: 0,
			},
		],
	};

	it("sums the uncommitted diff and reports the branch's own PR", () => {
		expect(
			buildWorkspaceRailMeta({
				branch: "feature/sidebar-recap",
				gitStatus: dirty,
				prStatus: {
					...noPr,
					number: 12,
					headBranch: "feature/sidebar-recap",
					state: "OPEN",
				},
			}),
		).toEqual({ prNumber: 12, prLabel: "PR", prState: "open", additions: 40, deletions: 2 });
	});

	it("labels drafts and GitLab merge requests", () => {
		expect(
			buildWorkspaceRailMeta({
				branch: "feature/x",
				gitStatus: cleanGitStatus,
				prStatus: {
					...noPr,
					provider: "gitlab",
					number: 7,
					headBranch: "feature/x",
					state: "opened",
					isDraft: true,
				},
			}),
		).toMatchObject({ prNumber: 7, prLabel: "MR", prState: "draft" });
	});

	it("ignores a PR opened from another branch", () => {
		expect(
			buildWorkspaceRailMeta({
				branch: "dcc/fresh-task",
				gitStatus: cleanGitStatus,
				prStatus: { ...noPr, number: 3, headBranch: "older/merged", state: "MERGED" },
			}),
		).toBeNull();
	});

	it("returns nothing when there is no PR and no diff", () => {
		expect(
			buildWorkspaceRailMeta({ branch: "main", gitStatus: cleanGitStatus, prStatus: noPr }),
		).toBeNull();
	});
});

describe("railNextActionKey", () => {
	it("stays silent where the row already says it", () => {
		for (const key of ["working", "workingClean", "prOpen", "clean"]) {
			expect(railNextActionKey(key)).toBeNull();
		}
		expect(railNextActionKey("mergeReady")).toBe("sidebar.nextAction.mergeReady");
		expect(railNextActionKey("deliveryFailure.push")).toBe(
			"sidebar.nextAction.deliveryFailure.push",
		);
	});

	it("keeps the short next actions in sync across locales", () => {
		const flatten = (value: unknown, prefix = ""): string[] =>
			value && typeof value === "object"
				? Object.entries(value).flatMap(([key, child]) =>
						flatten(child, prefix ? `${prefix}.${key}` : key),
					)
				: [prefix];
		expect(flatten(en.sidebar.nextAction).sort()).toEqual(
			flatten(ptBR.sidebar.nextAction).sort(),
		);
		for (const key of ["changes", "mergeReady", "checksFailing", "readyForPr", "conflicts_other"]) {
			expect(flatten(ptBR.sidebar.nextAction)).toContain(key);
		}
	});
});
