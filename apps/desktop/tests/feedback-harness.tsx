// Synthetic IPC only: this fixture never authenticates, opens URLs or creates real issues.
import { useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TooltipProvider } from "@/components/ui/tooltip";
import { FeedbackDialog } from "@/features/feedback/feedback-dialog";
import {
	feedbackApi,
	type FeedbackInput,
	type FeedbackIssue,
} from "@/features/feedback/feedback-api";
import { WorkspacesSidebar } from "@/features/workspaces/sidebar";
import i18n from "@/i18n/config";
import "@/styles/app.css";

const queryClient = new QueryClient({
	defaultOptions: { queries: { retry: false } },
});
const fixture = {
	login: "alice",
	offline: false,
	disconnected: false,
	uncertain: false,
	createCalls: [] as FeedbackInput[],
	external: [] as string[],
	settings: 0,
	issues: [
		{
			number: 42,
			title: "[Bug]: A barra lateral não reabre",
			category: "bug",
			state: "open",
			stateReason: null,
		},
		{
			number: 41,
			title: "[Feature]: Preservar o rascunho do feedback",
			category: "improvement",
			state: "closed",
			stateReason: "completed",
		},
		{
			number: 40,
			title: "[Feedback]: Ajustar uma integração",
			category: "other",
			state: "closed",
			stateReason: "not_planned",
		},
	].map((item) => ({
		...item,
		url: `https://github.com/wharley/DevCommandCenter/issues/${item.number}`,
		createdAt: "2026-09-11T12:00:00Z",
	})) as FeedbackIssue[],
};
Object.assign(window, {
	feedbackFixture: fixture,
	__TAURI_INTERNALS__: {
		invoke: async (command: string, args: { url?: string }) => {
			if (command === "shell_open_external") {
				fixture.external.push(args.url!);
				return { ok: true };
			}
			return [];
		},
	},
});
feedbackApi.context = async () => {
	if (fixture.disconnected) throw new Error("FEEDBACK_AUTH_REQUIRED");
	return {
		login: fixture.login,
		version: "0.1.70",
		platform: "macos",
		architecture: "aarch64",
	};
};
feedbackApi.list = async (login, page) => {
	if (fixture.offline) throw new Error("FEEDBACK_NETWORK_ERROR");
	return {
		issues: login === "alice" && page === 1 ? fixture.issues : [],
		hasNext: false,
	};
};
feedbackApi.create = async (input) => {
	fixture.createCalls.push(input);
	await new Promise((resolve) => setTimeout(resolve, 300));
	if (fixture.uncertain) throw new Error("FEEDBACK_UNCERTAIN");
	const issue: FeedbackIssue = {
		number: 43,
		title: `[Bug]: ${input.title}`,
		category: input.category,
		state: "open",
		stateReason: null,
		createdAt: new Date().toISOString(),
		url: "https://github.com/wharley/DevCommandCenter/issues/43",
	};
	fixture.issues = [
		issue,
		...fixture.issues.filter((item) => item.number !== 43),
	];
	return issue;
};

function Harness() {
	const [open, setOpen] = useState(false);
	const [collapsed, setCollapsed] = useState(false);
	const [dark, setDark] = useState(true);
	document.documentElement.classList.toggle("dark", dark);
	const noop = () => {};
	return (
		<TooltipProvider>
			<div className="flex h-screen bg-background text-foreground">
				<aside
					className={
						collapsed
							? "w-12 shrink-0 border-r border-border"
							: "w-64 shrink-0 border-r border-border"
					}
				>
					<WorkspacesSidebar
						collapsed={collapsed}
						repositories={[]}
						workspaces={[]}
						selectedWorkspaceId={null}
						onSelectWorkspace={noop}
						onNewTask={noop}
						onCreateWorkspace={noop}
						onCloneWorkspace={noop}
						onOpenSettings={() => {
							fixture.settings++;
						}}
						onOpenSkills={noop}
						onOpenUsage={noop}
						onOpenHelp={noop}
						onOpenPullRequests={noop}
						onToggleCollapsed={() => setCollapsed(!collapsed)}
						onOpenFeedback={() => setOpen(true)}
					/>
				</aside>
				<main className="flex flex-1 flex-col gap-6 p-10">
					<h1 className="text-xl">Dev Command Center</h1>
					<p className="text-sm text-muted-foreground">
						Seu espaço de trabalho.
					</p>
					<div className="mt-auto flex gap-4 text-xs">
						<button onClick={() => setDark(!dark)}>Trocar tema</button>
						<button
							onClick={() =>
								void i18n.changeLanguage(
									i18n.language === "en" ? "pt-BR" : "en",
								)
							}
						>
							Idioma
						</button>
					</div>
				</main>
				<FeedbackDialog
					open={open}
					onOpenChange={setOpen}
					onOpenSettings={() => {
						fixture.settings++;
					}}
				/>
			</div>
		</TooltipProvider>
	);
}
createRoot(document.getElementById("root")!).render(
	<QueryClientProvider client={queryClient}>
		<Harness />
	</QueryClientProvider>,
);
