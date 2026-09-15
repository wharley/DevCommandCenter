import { ActiveThreadViewport } from "@/features/panel/ActiveThreadViewport";
// Real review surfaces with synthetic in-memory IPC. Never reads or mutates a user repository.
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
win.demoStaged = [];
win.demoCommitted = false;
win.demoAhead = 1;
win.demoPr = false;
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
		(win.reviewRequests ??= []).push({ command, args });
		switch (command) {
			case "plugin:app|set_app_theme":
				return;
			case "workspace_git_status":
				return {
					staged: win.demoStaged,
					unstaged: win.emptyReview || win.demoCommitted ? [] : changes.filter((file) => !win.demoStaged.some((staged: { path: string }) => staged.path === file.path)),
					stagedFingerprint: JSON.stringify(win.demoStaged),
					currentBranch: head,
					aheadOfRemoteCount: win.demoAhead,
					behindOfRemoteCount: 0,
					conflictCount: 0,
					mergeInProgress: false,
				};
			case "workspace_forge_context":
				return { provider: "github", status: "ready", remoteState: "ok", remoteName: "origin", effectiveLogin: "demo" };
			case "workspace_pr_status":
				return { number: win.demoPr ? 42 : null, headBranch: win.demoPr ? head : null, baseBranch: "main", state: win.demoPr ? "OPEN" : null };
			case "workspace_git_stage_file":
				win.demoStaged = changes.filter((file) => file.path === args.input.relativePath);
				return;
			case "workspace_git_unstage_file":
				win.demoStaged = win.demoStaged.filter((file: { path: string }) => file.path !== args.input.relativePath);
				return;
			case "workspace_git_commit":
				win.demoStaged = []; win.demoCommitted = true; win.demoAhead += 1; return;
			case "workspace_git_push":
				win.demoAhead = 0; return;
			case "workspace_project_automation_config":
				return { beforePush: [], configHash: "demo" };
			case "workspace_change_request_create":
				win.demoPr = true; return;
			case "workspace_change_request_view_web":
				return;
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
				return win.emptyReview ? null : args.input.turnId === "turn-history" ? {
					...turn, turnId: "turn-history", snapshotId: "snapshot-history", insertions: 11, deletions: 3,
					files: [changes[0], { ...changes[0], path: "src/components/composer/use-prefill.ts", insertions: 7, deletions: 2 }, { ...changes[0], path: "docs/review-notes.md", status: "A", untracked: true, insertions: 3, deletions: 0 }, { ...changes[0], path: "assets/preview.png", insertions: 0, deletions: 0, previewUnavailable: true }],
				} : turn;
			case "turn_review_file_diff":
				return {
					snapshotId: args.input.snapshotId,
					path: args.input.path,
					diff: args.input.path === "docs/review-notes.md" ? "diff --git a/docs/review-notes.md b/docs/review-notes.md\nnew file mode 100644\n--- /dev/null\n+++ b/docs/review-notes.md\n@@ -0,0 +1,3 @@\n+# Review\n+\n+Keep the composer draft when reviewing changes.\n" : `diff --git a/${args.input.path} b/${args.input.path}\n--- a/${args.input.path}\n+++ b/${args.input.path}\n${patch}\n`,
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
function TimelineFixture() {
	const [inspectorOpen, setInspectorOpen] = useState(false);
	return <div className="flex min-h-0 min-w-0 flex-1">
		<div className="flex min-w-0 flex-1 flex-col">
			<header className="border-b border-border px-5 py-4 text-sm font-medium">Revisar o fluxo de anotações</header>
			<ActiveThreadViewport
				messages={[
					{ id: "user-history", role: "user", label: "User", content: "Corrigir o preenchimento da tarefa criada a partir da anotação." },
					{ id: "assistant-history", role: "assistant", label: "Assistant", content: "Corrigi o preenchimento do campo. A anotação continua aberta para você conferir o resultado antes de concluir.", turnId: "turn-history", turnSettled: true, streaming: false },
					{ id: "user-new", role: "user", label: "User", content: "Também ajuste o texto do botão de revisão." },
					{ id: "assistant-new", role: "assistant", label: "Assistant", content: "O botão agora mostra uma ação mais clara. Você pode conferir as alterações abaixo.", turnId: "turn-demo", turnSettled: true, streaming: false },
				]}
				hasLoaded isEmpty={false} workspaceName="Review fixture" sessionState="idle" lastTurnState="completed"
				pendingPrompt={null} workspacePath="/fixture/review" workspaceId="workspace-demo" sessionId="session-demo"
				planMessageId={null} planApproved={false} planReadOnly={false} activeMissionSpecRelativePath={null}
				activeMissionSpecHash={null} autoSaveMissionValidation={false} onSelectSession={() => {}} onOpenPlan={() => {}} onReviewChanges={() => setInspectorOpen(true)}
			/>
			<div className="px-5 pb-5 pt-2"><textarea aria-label="Mensagem" placeholder="Peça um ajuste ou continue a conversa…" className="min-h-24 w-full rounded-xl border border-border bg-background p-3 text-sm" /></div>
		</div>
		{inspectorOpen && <aside className="flex w-[380px] min-h-0 shrink-0 flex-col border-l border-border" aria-label="Inspector de revisão">
			<button className="self-end px-3 py-2 text-xs text-muted-foreground" onClick={() => setInspectorOpen(false)}>Fechar inspector</button>
			<InspectorChangesSection workspaceRoot="/fixture/review" workspaceId="workspace-demo" sessionId="session-demo" selectedPreview={null} onSelectPreview={() => {}} />
		</aside>}
	</div>;
}

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
				<button onClick={() => setSurface("timeline")}>Timeline</button>
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
				{surface === "timeline" ? <TimelineFixture /> : surface === "inspector" ? (
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
