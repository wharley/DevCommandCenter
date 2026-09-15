import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "@/lib/workspace-api";
import { TurnReviewDelivery } from "./turn-review-delivery";
import { TurnReviewCommitDialog } from "./turn-review-commit-dialog";
import { setWorkspaceDeliveryBusy } from "@/features/commit/workspace-delivery-busy";
import type { WorkspaceGitStatusOutput } from "@dcc/contracts";

vi.mock("@/lib/workspace-api", () => ({
	workspaceGitStatus: vi.fn(),
	workspaceGitStageFile: vi.fn(),
	workspaceGitUnstageFile: vi.fn(),
	workspaceGitCommit: vi.fn(),
	workspaceGitStageAll: vi.fn(),
	workspaceGitCommitPush: vi.fn(),
	workspaceGitCommitSuggestion: vi.fn(),
	workspaceGitPush: vi.fn(),
	workspaceGitSyncBase: vi.fn(),
	workspaceChangeRequestMerge: vi.fn(),
	workspaceChangeRequestViewWeb: vi.fn(),
	workspaceChangeRequestCreate: vi.fn(),
	workspaceProjectAutomationConfig: vi.fn(),
	workspaceRunProjectTasks: vi.fn(),
	workspaceForgeContext: vi.fn(),
	workspacePrStatus: vi.fn(),
	workspaceGitBranchDiff: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: { path?: string }) =>
			options?.path ? `${key}:${options.path}` : key,
	}),
}));
vi.mock("@/features/inspector/workspace-git-file-preview", () => ({
	WorkspaceGitFilePreview: ({
		selection,
	}: {
		selection: { path: string; group: string };
	}) => <div data-live-preview={`${selection.group}:${selection.path}`} />,
}));
const rootPath = "/fixture/workspace";
const file = {
	path: "src/current.ts",
	name: "current.ts",
	absolutePath: `${rootPath}/src/current.ts`,
	status: "M",
	insertions: 2,
	deletions: 1,
};
let status: WorkspaceGitStatusOutput;
let root: Root;
let container: HTMLDivElement;
let client: QueryClient;
const close = vi.fn();
const committed = vi.fn();
const settle = () => new Promise((resolve) => setTimeout(resolve, 25));
async function render(kind: "footer" | "commit" = "footer") {
	await act(async () => {
		root.render(
			<QueryClientProvider client={client}>
				{kind === "footer" ? (
					<TurnReviewDelivery workspaceRoot={rootPath} onReview={vi.fn()} />
				) : (
					<TurnReviewCommitDialog
						workspaceRoot={rootPath}
						onClose={close}
						onCommitted={committed}
					/>
				)}
			</QueryClientProvider>,
		);
	});
	await act(settle);
	await act(settle);
}
function button(label: string) {
	const found = [...document.querySelectorAll("button")].find(
		(el) => el.textContent === label || el.getAttribute("aria-label") === label,
	);
	expect(found, label).toBeTruthy();
	return found!;
}
async function click(label: string) {
	await act(async () => button(label).click());
	await act(settle);
}
beforeEach(() => {
	vi.resetAllMocks();
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	status = {
		staged: [file],
		unstaged: [{ ...file, path: "src/new.ts", name: "new.ts", status: "?" }],
		stagedFingerprint: "index-v1",
		currentBranch: "feature/review",
		aheadOfRemoteCount: 0,
		behindOfRemoteCount: 0,
		conflictCount: 0,
		mergeInProgress: false,
	};
	vi.mocked(api.workspaceGitStatus).mockImplementation(async () =>
		structuredClone(status),
	);
	vi.mocked(api.workspaceForgeContext).mockResolvedValue({
		provider: "github",
		status: "ready",
		remoteState: "ok",
		remoteName: "origin",
		effectiveLogin: "developer",
	} as Awaited<ReturnType<typeof api.workspaceForgeContext>>);
	vi.mocked(api.workspacePrStatus).mockResolvedValue({
		number: null,
		headBranch: null,
		state: null,
	} as Awaited<ReturnType<typeof api.workspacePrStatus>>);
	vi.mocked(api.workspaceGitBranchDiff).mockResolvedValue({
		changes: [file],
		baseBranch: "main",
	});
	vi.mocked(api.workspaceProjectAutomationConfig).mockResolvedValue({
		beforePush: [],
		configHash: "config-v1",
		setupCommand: null,
		tasks: [],
		beforeMerge: [],
		sourcePath: ".dcc/project.json",
		trackedInGit: true,
		deliveryPolicy: {
			minimumApprovals: 0,
			requirePipeline: false,
			requireResolvedDiscussions: false,
			requireCurrentBase: false,
			requireBeforeMergeChecks: false,
		},
	} as Awaited<ReturnType<typeof api.workspaceProjectAutomationConfig>>);
	committed.mockResolvedValue(undefined);
	client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});
afterEach(async () => {
	await act(async () => root.unmount());
	client.clear();
	container.remove();
	setWorkspaceDeliveryBusy(rootPath, false);
	vi.unstubAllGlobals();
});

describe("live commit review", () => {
	it("opens the current index without staging anything and commits only its fingerprint", async () => {
		await render("commit");
		expect(document.body.textContent).toContain("src/current.ts");
		expect(document.body.textContent).toContain("src/new.ts");
		expect(api.workspaceGitStageFile).not.toHaveBeenCalled();
		expect(api.workspaceGitStageAll).not.toHaveBeenCalled();
		await click("turnReview.delivery.confirmCommit");
		expect(api.workspaceGitCommit).toHaveBeenCalledWith(
			expect.objectContaining({
				workspaceRoot: rootPath,
				stagedFingerprint: "index-v1",
			}),
		);
		expect(api.workspaceGitPush).not.toHaveBeenCalled();
		expect(api.workspaceGitCommitPush).not.toHaveBeenCalled();
		expect(committed).toHaveBeenCalledTimes(1);
		expect(close).toHaveBeenCalledTimes(1);
	});
	it("stages only an explicitly selected file and refreshes the index", async () => {
		vi.mocked(api.workspaceGitStageFile).mockImplementation(async () => {
			status = {
				...status,
				staged: [...status.staged, status.unstaged[0]!],
				unstaged: [],
				stagedFingerprint: "index-v2",
			};
		});
		await render("commit");
		await click("turnReview.delivery.stageFile:src/new.ts");
		expect(api.workspaceGitStageFile).toHaveBeenCalledExactlyOnceWith({
			workspaceRoot: rootPath,
			relativePath: "src/new.ts",
		});
		await click("turnReview.delivery.confirmCommit");
		expect(api.workspaceGitCommit).toHaveBeenCalledWith(
			expect.objectContaining({ stagedFingerprint: "index-v2" }),
		);
	});
	it.each(["index", "branch"])(
		"rejects external %s changes until the user refreshes",
		async (change) => {
			await render("commit");
			if (change === "index") status.stagedFingerprint = "external-change";
			else status.currentBranch = "other-branch";
			await click("turnReview.delivery.confirmCommit");
			expect(api.workspaceGitCommit).not.toHaveBeenCalled();
			expect(document.querySelector('[role="alert"]')?.textContent).toContain(
				"turnReview.delivery.changed",
			);
			expect(close).not.toHaveBeenCalled();
		},
	);
	it("keeps an unsuccessful commit open and does not submit duplicate clicks", async () => {
		vi.mocked(api.workspaceGitCommit).mockRejectedValue(
			new Error("hook rejected commit"),
		);
		await render("commit");
		await act(async () => {
			button("turnReview.delivery.confirmCommit").click();
			button("turnReview.delivery.confirmCommit").click();
		});
		await act(settle);
		expect(api.workspaceGitCommit).toHaveBeenCalledTimes(1);
		expect(document.body.textContent).toContain("hook rejected commit");
		expect(close).not.toHaveBeenCalled();
	});
	it("does not stage automatically when nothing is prepared", async () => {
		status.staged = [];
		await render("commit");
		expect(button("turnReview.delivery.confirmCommit").disabled).toBe(true);
		expect(api.workspaceGitStageAll).not.toHaveBeenCalled();
	});
});

describe("review branch actions", () => {
	it("shows the live branch and keeps creating a PR behind pending pushes", async () => {
		status.aheadOfRemoteCount = 2;
		await render();
		expect(container.textContent).toContain("feature/review");
		expect(button("commit.modes.create-pr.idle").disabled).toBe(true);
		await click("commit.modes.push.idle");
		expect(api.workspaceGitPush).toHaveBeenCalledExactlyOnceWith({
			workspaceRoot: rootPath,
			forgeLogin: "developer",
		});
		expect(api.workspaceGitStageAll).not.toHaveBeenCalled();
	});
	it("opens an associated PR even with pending local changes", async () => {
		vi.mocked(api.workspacePrStatus).mockResolvedValue({
			number: 42,
			headBranch: "feature/review",
			state: "OPEN",
		} as Awaited<ReturnType<typeof api.workspacePrStatus>>);
		await render();
		await click("commit.modes.open-pr.idle #42");
		expect(api.workspaceChangeRequestViewWeb).toHaveBeenCalledTimes(1);
		expect(api.workspaceGitCommit).not.toHaveBeenCalled();
	});
	it("does not open a PR associated with another branch", async () => {
		vi.mocked(api.workspacePrStatus).mockResolvedValue({
			number: 42,
			headBranch: "other",
			state: "OPEN",
		} as Awaited<ReturnType<typeof api.workspacePrStatus>>);
		await render();
		expect(container.textContent).not.toContain("#42");
		expect(button("commit.modes.create-pr.idle").disabled).toBe(false);
	});
	it("creates a PR only from branch commits and executes the configured checks", async () => {
		vi.mocked(api.workspaceProjectAutomationConfig).mockResolvedValue({
			beforePush: ["lint"],
			configHash: "config-v1",
		} as Awaited<ReturnType<typeof api.workspaceProjectAutomationConfig>>);
		vi.mocked(api.workspaceRunProjectTasks).mockResolvedValue({
			report: { status: "passed" },
			changedFiles: false,
		} as Awaited<ReturnType<typeof api.workspaceRunProjectTasks>>);
		await render();
		await click("commit.modes.create-pr.idle");
		expect(document.body.textContent).not.toContain(
			"composer.executionDock.createRequest.includeLocalChanges",
		);
		await act(async () =>
			document
				.querySelector("form")!
				.dispatchEvent(
					new Event("submit", { bubbles: true, cancelable: true }),
				),
		);
		await act(settle);
		expect(api.workspaceRunProjectTasks).toHaveBeenCalledWith({
			workspaceRoot: rootPath,
			taskIds: ["lint"],
			expectedConfigHash: "config-v1",
		});
		expect(api.workspaceChangeRequestCreate).toHaveBeenCalledWith(
			expect.objectContaining({
				workspaceRoot: rootPath,
				title: "review",
				draft: false,
			}),
		);
		expect(api.workspaceGitStageAll).not.toHaveBeenCalled();
		expect(api.workspaceGitCommitPush).not.toHaveBeenCalled();
	});
	it("keeps the PR form and error visible when pre-push checks fail", async () => {
		vi.mocked(api.workspaceProjectAutomationConfig).mockResolvedValue({
			beforePush: ["test"],
			configHash: "config",
		} as Awaited<ReturnType<typeof api.workspaceProjectAutomationConfig>>);
		vi.mocked(api.workspaceRunProjectTasks).mockResolvedValue({
			report: { status: "failed" },
		} as Awaited<ReturnType<typeof api.workspaceRunProjectTasks>>);
		await render();
		await click("commit.modes.create-pr.idle");
		await act(async () =>
			document
				.querySelector("form")!
				.dispatchEvent(
					new Event("submit", { bubbles: true, cancelable: true }),
				),
		);
		await act(settle);
		expect(api.workspaceChangeRequestCreate).not.toHaveBeenCalled();
		expect(
			document.querySelector('[role="dialog"] [role="alert"]')?.textContent,
		).toContain("beforePushBlocked");
	});
	it("rejects a push after the checked-out branch changes", async () => {
		status.aheadOfRemoteCount = 1;
		await render();
		status.currentBranch = "other";
		await click("commit.modes.push.idle");
		expect(api.workspaceGitPush).not.toHaveBeenCalled();
		expect(container.textContent).toContain("turnReview.delivery.changed");
	});
	it("does not enable mutations when Git status fails", async () => {
		vi.mocked(api.workspaceGitStatus).mockRejectedValue(
			new Error("unavailable"),
		);
		await render();
		expect(button("composer.executionDock.actions.commit").disabled).toBe(true);
		expect(button("commit.modes.push.idle").disabled).toBe(true);
		expect(button("commit.modes.create-pr.idle").disabled).toBe(true);
	});
});
