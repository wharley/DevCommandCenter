// Browser fixture only. All IPC and task creation use isolated, synthetic data.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import { NotesWorkspace } from "@/features/notes/notes-workspace";
import { useProjectNotes } from "@/features/notes/use-project-notes";
import type { ProjectNote } from "@/features/notes/notes-api";
import { Toaster } from "sonner";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { WorkspacesSidebar } from "@/features/workspaces/sidebar";
import { NewTaskLaunchState } from "@/features/panel/NewTaskLaunchState";
import { AssistantMessage } from "@/features/panel/message-components/AssistantMessage";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
	DialogDescription,
	DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import type { Repository } from "@dcc/contracts";
import type { WorkspaceSummary } from "@/features/workspaces/types";
import { ExecutionContextRail } from "@/features/composer/ExecutionContextRail";
import "@/i18n/config";
import "@/styles/app.css";

const now = new Date().toISOString();
const restorationPreview = new URLSearchParams(location.search).has("restore");
const designPreview = new URLSearchParams(location.search).has("design");
const queryClient = new QueryClient({
	defaultOptions: { queries: { retry: false, enabled: false } },
});
const repositories: Repository[] = [
	{
		id: "repo-dcc",
		projectId: "project",
		name: "DCC",
		displayName: "Dev Command Center",
		icon: "terminal",
		color: "emerald",
		rootPath: "/fixture/dcc",
		baseBranch: "main",
	},
	{
		id: "repo-orbit",
		projectId: "other",
		name: "Orbit",
		displayName: "Orbit",
		icon: "globe",
		color: "violet",
		rootPath: "/fixture/orbit",
		baseBranch: "main",
	},
].map((repo) => ({
	pinnedAt: null,
	remote: null,
	remoteUrl: null,
	forgeProvider: null,
	forgeLogin: null,
	createdAt: now,
	updatedAt: now,
	...repo,
}));
const workspaces: WorkspaceSummary[] = [
	{
		id: "workspace",
		projectId: "project",
		name: "Implementar login",
		branch: "dcc/main",
		rootPath: "/fixture/dcc",
		status: "ready",
	},
	{
		id: "workspace-notes",
		projectId: "project",
		name: "Dar espaço às próximas ideias",
		branch: "dcc/notes",
		rootPath: "/fixture/dcc",
		status: "ready",
	},
	{
		id: "workspace-review",
		projectId: "project",
		name: "Revisar conexão remota",
		branch: "dcc/remote",
		rootPath: "/fixture/dcc",
		status: "setup_pending",
	},
	{
		id: "workspace-orbit",
		projectId: "other",
		name: "Busca por contexto",
		branch: "orbit/search",
		rootPath: "/fixture/orbit",
		status: "ready",
	},
	{
		id: "workspace-done",
		projectId: "project",
		name: "Ajustar atalhos do teclado",
		branch: "dcc/keys",
		rootPath: "/fixture/dcc",
		status: "completed",
	},
];
const seed: ProjectNote[] = [
	{
		id: "note-login",
		projectId: "project",
		projectName: "Dev Command Center",
		title: "Sessões sob controle",
		content:
			"Permitir consultar os dispositivos conectados e encerrar uma sessão individualmente. Um próximo passo para o fluxo de login.",
		contextSnapshot:
			"Ao implementar o login, discutimos listar os dispositivos conectados e revogar sessões individuais sem desconectar o dispositivo atual.",
		sourceSessionId: "session",
		sourceWorkspaceId: "workspace",
		sourceTaskTitle: "Implementar login",
		implementationTaskId: null,
		status: "open",
		color: "amber",
		pinned: false,
		revision: 1,
		createdAt: now,
		updatedAt: now,
		completedAt: null,
	},
	{
		id: "note-review",
		projectId: "project",
		projectName: "Dev Command Center",
		title: "Revisão sem perder o fio",
		content:
			"Guardar o ponto exato da revisão para retomar depois, mesmo quando alternamos entre projetos.",
		contextSnapshot: "",
		sourceSessionId: null,
		sourceWorkspaceId: null,
		sourceTaskTitle: "Melhorar a revisão",
		implementationTaskId: null,
		status: "open",
		color: "violet",
		pinned: false,
		revision: 1,
		createdAt: now,
		updatedAt: now,
		completedAt: null,
	},
	{
		id: "note-other",
		projectId: "other",
		projectName: "Orbit",
		title: "Uma busca que entende o contexto",
		content:
			"Explorar filtros por conversa e projeto, mantendo as ideias próximas do trabalho.",
		contextSnapshot: "",
		sourceSessionId: null,
		sourceWorkspaceId: null,
		sourceTaskTitle: "",
		implementationTaskId: null,
		status: "open",
		color: "mint",
		pinned: false,
		revision: 1,
		createdAt: now,
		updatedAt: now,
		completedAt: null,
	},
];
const win = window as unknown as {
	__TAURI_INTERNALS__: {
		invoke: (command: string, args?: any) => Promise<any>;
	};
	notesFailSave?: boolean;
	notesTaskCalls?: number;
};
const read = (): ProjectNote[] =>
	JSON.parse(localStorage.getItem("fixture.notes") ?? JSON.stringify(seed));
const write = (notes: ProjectNote[]) =>
	localStorage.setItem("fixture.notes", JSON.stringify(notes));
win.__TAURI_INTERNALS__ = {
	invoke: async (command, args) => {
		const notes = read();
		if (command === "list_project_notes") {
			if (restorationPreview)
				await new Promise((resolve) => setTimeout(resolve, 500));
			return notes;
		}
		if (command === "create_project_note") {
			const note = {
				...seed[0],
				...args.input,
				id: crypto.randomUUID(),
				contextSnapshot: args.input.contextSnapshot,
				revision: 1,
				color: "sky",
			};
			write([note, ...notes]);
			return note;
		}
		if (command === "update_project_note") {
			if (win.notesFailSave) throw new Error("fixture_failure");
			const current = notes.find((note) => note.id === args.input.id);
			if (!current || current.revision !== args.input.revision)
				throw new Error("note_conflict");
			const note = {
				...current,
				...args.input,
				revision: current.revision + 1,
			};
			write(notes.map((item) => (item.id === note.id ? note : item)));
			return note;
		}
		if (command === "delete_project_notes") {
			write(notes.filter((note) => !args.ids.includes(note.id)));
			return;
		}
		throw new Error(`Unexpected fixture IPC: ${command}`);
	},
};

function Harness() {
	const notes = useProjectNotes();
	const [open, setOpen] = useState(!designPreview && !restorationPreview);
	const [launch, setLaunch] = useState(designPreview);
	const [collapsed, setCollapsed] = useState(false);
	const [settings, setSettings] = useState(false);
	const [selectedWorkspace, setSelectedWorkspace] = useState("workspace");
	const [previewWorkspaces, setPreviewWorkspaces] = useState(workspaces);
	const [creatingTask, setCreatingTask] = useState(false);
	const [branch, setBranch] = useState("dcc/main");
	const [dark, setDark] = useState(true);
	const [project, setProject] = useState("project");
	const [draft, setDraft] = useState("");
	const [completionTaskIds, setCompletion] = useState<string[]>([]);
	document.documentElement.classList.toggle("dark", dark);
	return (
		<TooltipProvider>
			<div className="flex h-screen bg-background text-foreground">
				{designPreview ? (
					<aside
						className={
							collapsed
								? "w-12 shrink-0 border-r border-border"
								: "w-72 shrink-0 border-r border-border"
						}
					>
						<WorkspacesSidebar
							collapsed={collapsed}
							repositories={repositories}
							workspaces={previewWorkspaces}
							selectedWorkspaceId={launch ? null : selectedWorkspace}
							newTaskActive={launch}
							showAgentStates={false}
							showCompletedDiskUsage={false}
							onSelectWorkspace={(id) => {
								setSelectedWorkspace(id);
								setProject(
									workspaces.find((workspace) => workspace.id === id)
										?.projectId ?? "project",
								);
								setLaunch(false);
							}}
							onNewTask={() => setLaunch(true)}
							onCreateWorkspace={() => setSettings(true)}
							onCloneWorkspace={() => setSettings(true)}
							onOpenSettings={() => setSettings(true)}
							onOpenSkills={() => setSettings(true)}
							onOpenUsage={() => setSettings(true)}
							onOpenHelp={() => setSettings(true)}
							onOpenPullRequests={() => setSettings(true)}
							onToggleCollapsed={() => setCollapsed((value) => !value)}
							onOpenNotes={() => setOpen(true)}
							notesCount={
								notes.notes.filter((note) => note.status === "open").length
							}
							onRenameWorkspace={async (id, name) =>
								setPreviewWorkspaces((rows) =>
									rows.map((row) => (row.id === id ? { ...row, name } : row)),
								)
							}
							onCompleteWorkspace={async (id) =>
								setPreviewWorkspaces((rows) =>
									rows.map((row) =>
										row.id === id ? { ...row, status: "completed" } : row,
									),
								)
							}
							onRestoreWorkspace={(id) =>
								setPreviewWorkspaces((rows) =>
									rows.map((row) =>
										row.id === id ? { ...row, status: "ready" } : row,
									),
								)
							}
						/>
					</aside>
				) : (
					<aside className="w-64 border-r border-border p-6">
						<p className="mb-10 text-sm font-semibold">DEV COMMAND CENTER</p>
						<button onClick={() => setOpen(true)} className="mb-6 block">
							Anotações
						</button>
						<button onClick={() => setDark(!dark)} className="mb-6 block">
							Trocar tema
						</button>
						<button
							onClick={() =>
								setProject(project === "project" ? "other" : "project")
							}
							className="mb-6 block"
						>
							Trocar projeto
						</button>
						{restorationPreview ? (
							<button
								className="mb-6 block"
								onClick={() =>
									setSelectedWorkspace((value) =>
										value === "workspace" ? "workspace-notes" : "workspace",
									)
								}
							>
								Trocar tarefa
							</button>
						) : null}
						<button onClick={() => setCompletion(["task-from-note"])}>
							Concluir tarefa criada
						</button>
					</aside>
				)}
				{designPreview && launch ? (
					<NewTaskLaunchState
						repositories={repositories}
						isCreating={creatingTask}
						onSelectProject={async (repo, mode) => {
							setCreatingTask(true);
							(window as any).designTaskCalls =
								((window as any).designTaskCalls ?? 0) + 1;
							(window as any).designTaskMode = mode;
							await new Promise((resolve) => setTimeout(resolve, 350));
							setSelectedWorkspace(
								repo.projectId === "other" ? "workspace-orbit" : "workspace",
							);
							setProject(repo.projectId);
							setCreatingTask(false);
							setLaunch(false);
						}}
						onSelectMultiple={() => setSettings(true)}
						onOpenProject={() => setSettings(true)}
					/>
				) : (
					<main className="flex min-w-0 flex-1 flex-col p-12">
						<span className="text-xs text-muted-foreground">
							WORKSPACE / LOGIN
						</span>
						<h1 className="mt-3 text-2xl">Implementar login</h1>
						<p
							id="capture-text"
							className="mt-10 max-w-lg text-sm leading-7 text-muted-foreground"
						>
							Ao implementar o login, surgiu a ideia de listar os dispositivos
							conectados e permitir revogar sessões individualmente.
						</p>
						{designPreview ? (
							<div className="mt-8 max-w-2xl">
								<AssistantMessage
									sessionId="session"
									content="Podemos preservar o contexto da conversa em uma anotação independente. Assim, você retoma a ideia quando fizer sentido."
									streaming
									annotations={[
										{
											id: "activity-preview",
											type: "commentary",
											content:
												"Conferindo o espaço da branch e os estados de interação.",
											streaming: true,
										},
									]}
								/>
							</div>
						) : null}
						<div
							className={
								designPreview
									? "dcc-composer-surface mt-auto rounded-2xl border border-border p-4"
									: "contents"
							}
						>
							<textarea
								aria-label="Composer de teste"
								value={draft}
								onChange={(event) => setDraft(event.target.value)}
								className={
									designPreview
										? "h-24 w-full resize-none bg-transparent text-sm outline-none"
										: "mt-14 h-48 w-full max-w-xl rounded-xl border border-border p-4"
								}
							/>
							{designPreview ? (
								<div className="flex items-center justify-between text-xs text-muted-foreground">
									<span>Claude · Sonnet</span>
									<Button size="sm">Enviar</Button>
								</div>
							) : null}
						</div>
						<div className={designPreview ? "pt-2" : "mt-auto pt-4"}>
							<ExecutionContextRail
								projectLabel="DCC"
								baseBranch="main"
								currentBranch={branch}
								isIsolatedWorkspace
								noteSessionId="session"
							/>
						</div>
					</main>
				)}
			</div>
			{designPreview ? (
				<div className="fixed right-4 top-3 flex gap-2 text-xs">
					<button onClick={() => setDark(!dark)}>Trocar tema</button>
					<button
						onClick={() =>
							setBranch((value) =>
								value === "dcc/main"
									? "dcc/feat-preservar-contexto-da-conversa-apos-commit"
									: "dcc/main",
							)
						}
					>
						Trocar branch
					</button>
				</div>
			) : null}
			<Dialog open={settings} onOpenChange={setSettings}>
				<DialogContent>
					<DialogHeader>
						<DialogTitle>Seu espaço de trabalho</DialogTitle>
						<DialogDescription>
							Uma prévia dos componentes compartilhados do DCC.
						</DialogDescription>
					</DialogHeader>
					<p className="text-sm leading-6 text-muted-foreground">
						Bordas suaves, foco visível e superfícies consistentes em cada
						etapa.
					</p>
					<DialogFooter>
						<Button onClick={() => setSettings(false)}>Concluído</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
			<NotesWorkspace
				controller={notes}
				open={open}
				onOpenChange={setOpen}
				scope={
					designPreview && launch
						? null
						: {
								projectId: project,
								projectName:
									project === "project" ? "Dev Command Center" : "Orbit",
								workspaceId: selectedWorkspace,
								sessionId: "session",
								taskTitle: "Implementar login",
							}
				}
				projects={[
					{ id: "project", name: "Dev Command Center" },
					{ id: "other", name: "Orbit" },
				]}
				onUse={(text) => setDraft((value) => value + text)}
				onCreateTask={async (note) => {
					win.notesTaskCalls = (win.notesTaskCalls ?? 0) + 1;
					setDraft(note.content);
					return "task-from-note";
				}}
				completionTaskIds={completionTaskIds}
				onCompletionHandled={() => setCompletion([])}
			/>
			<Toaster />
		</TooltipProvider>
	);
}
createRoot(document.getElementById("root")!).render(
	<QueryClientProvider client={queryClient}>
		<Harness />
	</QueryClientProvider>,
);
