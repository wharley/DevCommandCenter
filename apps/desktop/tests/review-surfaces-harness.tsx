// Real review surfaces with synthetic, read-only IPC. Never reads a user repository.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeProvider, useAppearance } from "@/components/theme-provider";
import { InspectorChangesSection } from "@/features/inspector/inspector-changes-section";
import { PullRequestsHub } from "@/features/pull-requests/pull-requests-hub";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";
import type {
	PullRequestHubItem,
	PullRequestHubDetailOutput,
	TurnReviewSummary,
} from "@dcc/contracts";
import "@/i18n/config";
import "@/styles/app.css";

const win = window as any;
const client = new QueryClient({
	defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
});
win.reviewIpc = [];
win.emptyReview = false;
localStorage.setItem("dcc.settings.coderabbit.integration-enabled", "false");
const head =
	"feature/review-workspace-changes-and-preserve-complete-branch-context";
const patch =
	"@@ -1,2 +1,2 @@\n-export const review = false;\n+export const review = true;\n export const version = 1;";
const changes = [
	{
		path: "src/review.ts",
		name: "review.ts",
		absolutePath: "/fixture/review/src/review.ts",
		status: "M",
		insertions: 1,
		deletions: 1,
	},
];
const turn: TurnReviewSummary = {
	snapshotId: "snapshot-demo",
	sessionId: "session-demo",
	turnId: "turn-demo",
	workspaceId: "workspace-demo",
	state: "available",
	compatibility: "compatible",
	baseFingerprint: "base",
	resultFingerprint: "result",
	files: changes,
	insertions: 1,
	deletions: 1,
	diffTruncated: false,
	excludedPreexistingUntrackedCount: 0,
	observedValidations: [],
	turnOutcome: "completed",
	outcomeReason: null,
	error: null,
	completedAt: null,
	guardedUndo: null,
	activeUndo: null,
};
const actor = { login: "demo-author", name: "Demo Author", avatarUrl: null, htmlUrl: null };
const items: PullRequestHubItem[] = [1, 2, 3].map((number) => ({
	id: `pr-${number}`,
	provider: "github",
	host: "github.com",
	repositoryId: "repo-demo",
	projectId: "project-demo",
	repositoryName: "example/studio",
	repositoryRoot: "/fixture/review",
	forgeLogin: "demo-reviewer",
	number,
	title: [
		"Preservar o contexto durante a revisão de mudanças",
		"Melhorar a navegação por teclado",
		"Documentar os estados de sincronização",
	][number - 1],
	body: "Esta mudança reúne o contexto da revisão e mantém a comparação entre branches legível.\n\nAs ações existentes continuam disponíveis durante a análise do código.",
	url: "https://example.invalid/pr",
	author: actor,
	headBranch: head,
	baseBranch: "main",
	state: "open",
	isDraft: false,
	reviewDecision: null,
	reviewRequestedForViewer: number === 1,
	createdByViewer: number === 2,
	reviewers: [actor],
	additions: 1,
	deletions: 1,
	changedFiles: 1,
	commentCount: 0,
	checksState: "success",
	updatedAt: new Date().toISOString(),
	linkedWorkspaceId: "workspace-demo",
	linkedWorkspaceName: "Demo",
}));
const detail: PullRequestHubDetailOutput = {
	body: items[0].body,
	comments: [],
	checks: [{ name: "TypeScript & unit tests", state: "success", detailsUrl: null }],
	files: [
		{
			path: "src/review.ts",
			previousPath: null,
			status: "modified",
			additions: 1,
			deletions: 1,
			patch,
			blobUrl: null,
		},
	],
	inlineComments: [],
	reviewCapabilities: {
		inlineComments: true,
		approve: true,
		requestChanges: true,
		replyToThreads: true,
		resolveThreads: true,
	},
	mergeCapabilities: {
		allowedMethods: ["merge", "squash"],
		viewerCanMerge: true,
		viewerPermission: "WRITE",
		mergeable: "MERGEABLE",
		mergeStateStatus: "CLEAN",
		headSha: "demo-sha",
	},
};
win.__TAURI_INTERNALS__ = {
	invoke: async (command: string, args: any) => {
		win.reviewIpc.push(command);
		switch (command) {
			case "plugin:app|set_app_theme":
				return;
			case "workspace_git_status":
				return {
					staged: [],
					unstaged: win.emptyReview ? [] : changes,
					stagedFingerprint: "demo",
					currentBranch: head,
					aheadOfRemoteCount: 0,
					behindOfRemoteCount: 0,
					conflictCount: 0,
					mergeInProgress: false,
				};
			case "workspace_git_branch_diff":
				return { changes: win.emptyReview ? [] : changes, baseBranch: "main" };
			case "workspace_git_file_preview_content":
				return {
					originalText:
						"export const review = false;\nexport const version = 1;\n",
					modifiedText:
						"export const review = true;\nexport const version = 1;\n",
					inline: false,
				};
			case "last_turn_review":
				return win.emptyReview ? null : turn;
			case "turn_review_file_diff":
				return {
					snapshotId: turn.snapshotId,
					path: "src/review.ts",
					diff: `diff --git a/src/review.ts b/src/review.ts\n--- a/src/review.ts\n+++ b/src/review.ts\n${patch}\n`,
					previewUnavailable: false,
				};
			case "pull_request_hub_list":
				return { items: win.emptyReview ? [] : items, warnings: [] };
			case "pull_request_hub_detail":
				return detail;
			default:
				throw Error("Unexpected fixture IPC: " + command);
		}
	},
};
function Fixture() {
	const [surface, setSurface] = useState("inspector");
	const [preview, setPreview] = useState<WorkspaceGitPreviewSelection | null>(
		null,
	);
	const { theme, setTheme } = useAppearance();
	return (
		<div style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
			<nav
				style={{
					display: "flex",
					flexWrap: "wrap",
					gap: 16,
					padding: 12,
					borderBottom: "1px solid var(--border)",
				}}
			>
				<button onClick={() => setSurface("inspector")}>Inspector</button>
				<button onClick={() => setSurface("pr")}>Pull Requests</button>
				<button onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
					Tema
				</button>
				<button
					onClick={() => {
						win.emptyReview = !win.emptyReview;
						void client.invalidateQueries();
					}}
				>
					Alternar vazio
				</button>
			</nav>
			<main
				style={{
					flex: 1,
					minHeight: 0,
					display: "flex",
					justifyContent: "flex-end",
				}}
			>
				{surface === "inspector" ? (
					<div
						style={{
							width: "min(100%, 380px)",
							display: "flex",
							minHeight: 0,
							borderLeft: "1px solid var(--border)",
						}}
					>
						<InspectorChangesSection
							workspaceRoot="/fixture/review"
							workspaceId="workspace-demo"
							sessionId="session-demo"
							selectedPreview={preview}
							onSelectPreview={setPreview}
						/>
					</div>
				) : (
					<div style={{ width: "100%", minWidth: 0 }}>
						<PullRequestsHub
							onOpenWorkspace={() => {}}
							onWorkOnPullRequest={async () => {}}
							providers={[]}
							selectedProviderId={null}
							selectedModelId={null}
							selectedProviderRuntime={null}
							onSelectProvider={() => {}}
							onSelectModel={() => {}}
						/>
					</div>
				)}
			</main>
		</div>
	);
}
createRoot(document.getElementById("root")!).render(
	<QueryClientProvider client={client}>
		<ThemeProvider>
			<TooltipProvider>
				<Fixture />
			</TooltipProvider>
		</ThemeProvider>
	</QueryClientProvider>,
);
