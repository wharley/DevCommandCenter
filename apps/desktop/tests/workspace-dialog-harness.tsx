// Browser fixture: no repositories are opened, cloned, or written.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { Toaster } from "sonner";
import { ThemeProvider, useAppearance } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { CreateWorkspaceDialog } from "@/features/workspaces/create-workspace-dialog";
import type { Repository } from "@dcc/contracts";
import "@/i18n/config";
import "@/styles/app.css";
const win = window as any;
win.workspaceCalls = [];
const scenario = new URLSearchParams(location.search).get("case");
const branch =
	"feature/review-the-complete-project-configuration-and-long-branch-context";
const repositories = [
	"Studio",
	"Checkout",
	"Documentação e convenções do projeto",
].map((name, i) => ({
	id: "repo-" + i,
	projectId: "project-" + i,
	name,
	displayName: null,
	icon: null,
	color: null,
	pinnedAt: null,
	rootPath: "/fixture/projects/" + name,
	baseBranch: i === 0 ? branch : "main",
	remote: null,
	createdAt: "2026-09-09T00:00:00Z",
	updatedAt: "2026-09-09T00:00:00Z",
})) as Repository[];
const context = {
	projectId: "project-0",
	workspaceRoot:
		"/fixture/projects/a-long-project-location/with-additional-directory-context/studio",
	label: "Studio",
};
win.__TAURI_INTERNALS__ = {
	invoke: async (command: string, args: any) => {
		if (command === "plugin:app|set_app_theme") return;
		win.workspaceCalls.push({ command, args });
		if (command === "list_local_branches") {
			if (scenario === "branch-error")
				throw Error("Falha de leitura de demonstração.");
			return { branches: [branch, "main", "develop"] };
		}
		if (command === "plugin:dialog|open")
			return scenario === "picker-cancel" ? null : "/fixture/new-project";
		if (command === "resolve_workspace_source_url")
			return {
				kind: "branch",
				url: args.input.url,
				provider: "github",
				host: "github.com",
				repository: "org/studio",
				headBranch: branch,
				headSha: "abc",
				baseBranch: "main",
				changeRequestNumber: null,
				title: null,
				author: null,
				state: null,
				sourceRepository: null,
				isCrossRepository: false,
			};
		throw Error("Unexpected fixture IPC: " + command);
	},
};
function Fixture() {
	const [open, setOpen] = useState(false);
	const [busy, setBusy] = useState(false);
	const { theme, setTheme } = useAppearance();
	const mutation = (kind: string) => async (input: any) => {
		win.workspaceCalls.push({ command: kind, args: input });
		setBusy(true);
		try {
			await new Promise((r) => setTimeout(r, 600));
			if (scenario === "submit-error")
				throw Error("Falha de criação de demonstração.");
			return {
				setupReport: { status: "pending" },
				setupHints: [],
				workspaces: [{}, {}],
			} as any;
		} finally {
			setBusy(false);
		}
	};
	return (
		<div style={{ padding: 20 }}>
			<button onClick={() => setOpen(true)}>Abrir modal</button>
			<button
				style={{ marginLeft: 20 }}
				onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
			>
				Tema
			</button>
			<CreateWorkspaceDialog
				open={open}
				onOpenChange={setOpen}
				mode={
					scenario === "clone" || scenario === "submit-error" ? "clone" : "open"
				}
				repositoryContext={scenario === "context" ? context : null}
				repositories={
					scenario === "empty" || scenario === "picker-cancel"
						? []
						: repositories
				}
				isSubmitting={busy}
				onCreateWorkspace={mutation("create")}
				onCloneWorkspace={mutation("clone")}
				onCreateWorkspaceBundle={mutation("bundle")}
				onCreateWorkspaceFromSourceUrl={mutation("source")}
			/>
			<Toaster />
		</div>
	);
}
createRoot(document.getElementById("root")!).render(
	<ThemeProvider>
		<TooltipProvider>
			<Fixture />
		</TooltipProvider>
	</ThemeProvider>,
);
