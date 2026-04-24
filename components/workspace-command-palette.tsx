"use client";

import { useMemo } from "react";
import {
  Bell,
  Bot,
  ChevronLeft,
  ChevronRight,
  FolderGit2,
  GitPullRequest,
  ListPlus,
  PanelLeftOpen,
  Monitor,
  Moon,
  Settings2,
  SunMedium,
  Terminal,
  Workflow,
  WandSparkles,
  Clock3,
  FileText,
} from "lucide-react";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from "@/components/ui/command";
import type {
  Comb,
  Pane,
  Project,
  ProjectRepoConfig,
  RepoTaskDefinition,
  RepoTaskTemplate,
} from "@/lib/database/types";

interface WorkspaceCommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projects: Project[];
  combs: Comb[];
  panes: Pane[];
  activeProject: Project | null;
  activeCombId: string | null;
  activePaneId: string | null;
  repoConfig: ProjectRepoConfig | null;
  /** Templates Markdown em `.dcc/tasks` (repo). */
  taskTemplates: RepoTaskTemplate[];
  tasks: RepoTaskDefinition[];
  onOpenSettings: () => void;
  onOpenNewWorkspace: () => void;
  onOpenBaseTerminal: () => void;
  onOpenWorkspaceTerminal: () => void;
  onOpenNewAgent: () => void;
  onOpenReview: () => void;
  onOpenRepoConfig: () => void;
  onOpenNotifications: () => void;
  currentTheme: "dark" | "light" | "system";
  onSetTheme: (theme: "dark" | "light" | "system") => void;
  onToggleTheme: () => void;
  onSelectProject: (projectId: string) => void;
  onSelectWorkspace: (combId: string) => void;
  onSelectPane: (paneId: string) => void;
  /** Histórico de navegação entre worktrees (⌘[ / ⌘]) */
  canGoBackWorktree?: boolean;
  canGoForwardWorktree?: boolean;
  onWorktreeHistoryBack?: () => void;
  onWorktreeHistoryForward?: () => void;
  onLaunchCommand: (payload: {
    title: string;
    command: string;
    cwdMode?: "project" | "worktree";
    description?: string | null;
  }) => void;
}

export function WorkspaceCommandPalette({
  open,
  onOpenChange,
  projects,
  combs,
  panes,
  activeProject,
  activeCombId,
  activePaneId,
  repoConfig,
  taskTemplates,
  tasks,
  onOpenSettings,
  onOpenNewWorkspace,
  onOpenBaseTerminal,
  onOpenWorkspaceTerminal,
  onOpenNewAgent,
  onOpenReview,
  onOpenRepoConfig,
  onOpenNotifications,
  currentTheme,
  onSetTheme,
  onToggleTheme,
  onSelectProject,
  onSelectWorkspace,
  onSelectPane,
  canGoBackWorktree = false,
  canGoForwardWorktree = false,
  onWorktreeHistoryBack,
  onWorktreeHistoryForward,
  onLaunchCommand,
}: WorkspaceCommandPaletteProps) {
  const activeProjectCombs = useMemo(
    () => (activeProject ? combs.filter((comb) => comb.projectId === activeProject.id) : []),
    [activeProject, combs],
  );
  const activeProjectPanes = useMemo(
    () =>
      activeProject
        ? panes.filter((pane) => activeProjectCombs.some((comb) => comb.id === pane.combId))
        : [],
    [activeProject, activeProjectCombs, panes],
  );

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Palette de comandos do workspace"
      description="Navegue por projetos, workspaces, panes e ações de repo"
      className="sm:max-w-3xl"
    >
      <CommandInput placeholder="Pesquisar ação, workspace, pane ou comando..." />
      <CommandList>
        <CommandEmpty>Nada encontrado.</CommandEmpty>

        <CommandGroup heading="Global">
          <CommandItem
            value="novo workspace"
            onSelect={() => {
              onOpenNewWorkspace();
              onOpenChange(false);
            }}
          >
            <ListPlus className="h-4 w-4" />
            Novo workspace
            <CommandShortcut>⌘⇧N</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="novo terminal"
            onSelect={() => {
              onOpenWorkspaceTerminal();
              onOpenChange(false);
            }}
          >
            <Terminal className="h-4 w-4" />
            Abrir terminal do workspace
            <CommandShortcut>⌘⇧T</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="novo agente"
            onSelect={() => {
              onOpenNewAgent();
              onOpenChange(false);
            }}
          >
            <Bot className="h-4 w-4" />
            Abrir agente CLI
            <CommandShortcut>⌘⇧A</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="review diff revisão"
            disabled={!activeCombId}
            onSelect={() => {
              onOpenReview();
              onOpenChange(false);
            }}
          >
            <GitPullRequest className="h-4 w-4" />
            Abrir review do workspace
            <CommandShortcut>⌘⇧V</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="terminal base"
            onSelect={() => {
              onOpenBaseTerminal();
              onOpenChange(false);
            }}
          >
            <FolderGit2 className="h-4 w-4" />
            Abrir terminal base
            <CommandShortcut>⌘⇧B</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="configurar repo"
            onSelect={() => {
              onOpenRepoConfig();
              onOpenChange(false);
            }}
          >
            <Workflow className="h-4 w-4" />
            Configurar repositório
            <CommandShortcut>⌘⇧R</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="notificações"
            onSelect={() => {
              onOpenNotifications();
              onOpenChange(false);
            }}
          >
            <Bell className="h-4 w-4" />
            Notificações
            <CommandShortcut>⌘⇧I</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="providers settings configurações"
            onSelect={() => {
              onOpenSettings();
              onOpenChange(false);
            }}
          >
            <Settings2 className="h-4 w-4" />
            Abrir providers
            <CommandShortcut>⌘⇧P</CommandShortcut>
          </CommandItem>
        </CommandGroup>

        <CommandSeparator />

        <CommandGroup heading="Histórico de workspaces">
          <CommandItem
            value="workspace anterior voltar histórico"
            disabled={!canGoBackWorktree}
            onSelect={() => {
              onWorktreeHistoryBack?.();
              onOpenChange(false);
            }}
          >
            <ChevronLeft className="h-4 w-4" />
            Workspace anterior
            <CommandShortcut>⌘[</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="workspace seguinte avançar histórico"
            disabled={!canGoForwardWorktree}
            onSelect={() => {
              onWorktreeHistoryForward?.();
              onOpenChange(false);
            }}
          >
            <ChevronRight className="h-4 w-4" />
            Workspace seguinte
            <CommandShortcut>⌘]</CommandShortcut>
          </CommandItem>
        </CommandGroup>

        <CommandSeparator />

        <CommandGroup heading="Temas">
          <CommandItem
            value={`tema alternar ${currentTheme}`}
            onSelect={() => {
              onToggleTheme();
              onOpenChange(false);
            }}
          >
            <Monitor className="h-4 w-4" />
            Alternar tema
            <CommandShortcut>⌘⌥T</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="tema escuro dark"
            onSelect={() => {
              onSetTheme("dark");
              onOpenChange(false);
            }}
          >
            <Moon className="h-4 w-4" />
            Tema escuro
            <CommandShortcut>⌘⇧D</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="tema claro light"
            onSelect={() => {
              onSetTheme("light");
              onOpenChange(false);
            }}
          >
            <SunMedium className="h-4 w-4" />
            Tema claro
            <CommandShortcut>⌘⇧L</CommandShortcut>
          </CommandItem>
          <CommandItem
            value="tema sistema system"
            onSelect={() => {
              onSetTheme("system");
              onOpenChange(false);
            }}
          >
            <Monitor className="h-4 w-4" />
            Tema do sistema
            <CommandShortcut>⌘⇧S</CommandShortcut>
          </CommandItem>
        </CommandGroup>

        <CommandSeparator />

        <CommandGroup heading="Projetos">
          {projects.map((project) => (
            <CommandItem
              key={project.id}
              value={`projeto ${project.name} ${project.path}`}
              onSelect={() => {
                onSelectProject(project.id);
                onOpenChange(false);
              }}
            >
              <FolderGit2 className="h-4 w-4" />
              {project.name}
            </CommandItem>
          ))}
        </CommandGroup>

        <CommandSeparator />

        <CommandGroup heading="Workspaces">
          {combs.map((comb) => (
            <CommandItem
              key={comb.id}
              value={`workspace ${comb.name} ${comb.branch ?? comb.baseBranch}`}
              onSelect={() => {
                onSelectWorkspace(comb.id);
                onOpenChange(false);
              }}
            >
              <PanelLeftOpen className="h-4 w-4" />
              {comb.name}
            </CommandItem>
          ))}
        </CommandGroup>

        <CommandSeparator />

        <CommandGroup heading="Panes ativos">
          {activeProjectPanes.map((pane) => (
            <CommandItem
              key={pane.id}
              value={`pane ${pane.title ?? pane.id} ${pane.type}`}
              onSelect={() => {
                onSelectPane(pane.id);
                onOpenChange(false);
              }}
            >
              <Terminal className="h-4 w-4" />
              {pane.title ?? pane.id}
            </CommandItem>
          ))}
        </CommandGroup>

        <CommandSeparator />

        <CommandGroup heading="Processos gerenciados">
          {(repoConfig?.processes ?? []).map((process) => (
            <CommandItem
              key={process.id}
              value={`processo ${process.name} ${process.command}`}
              onSelect={() => {
                onLaunchCommand({
                  title: process.name,
                  command: process.command,
                  cwdMode: process.cwdMode ?? "worktree",
                  description: process.description ?? null,
                });
                onOpenChange(false);
              }}
            >
              <Workflow className="h-4 w-4" />
              {process.name}
            </CommandItem>
          ))}
        </CommandGroup>

        <CommandSeparator />

        <CommandGroup heading="Presets rápidos">
          {(repoConfig?.presets ?? []).map((preset) => (
            <CommandItem
              key={preset.id}
              value={`preset ${preset.name} ${preset.command}`}
              onSelect={() => {
                onLaunchCommand({
                  title: preset.name,
                  command: preset.command,
                  cwdMode: "worktree",
                  description: preset.description ?? null,
                });
                onOpenChange(false);
              }}
            >
              <WandSparkles className="h-4 w-4" />
              {preset.name}
            </CommandItem>
          ))}
        </CommandGroup>

        {taskTemplates.length > 0 ? (
          <>
            <CommandSeparator />
            <CommandGroup heading="Templates de tarefas (.dcc/tasks)">
              {taskTemplates.map((tpl) => (
                <CommandItem
                  key={tpl.id}
                  value={`template task ${tpl.name} ${tpl.id} ${tpl.command}`}
                  onSelect={() => {
                    onLaunchCommand({
                      title: tpl.name,
                      command: tpl.command,
                      cwdMode: tpl.cwdMode ?? "worktree",
                      description: tpl.description ?? null,
                    });
                    onOpenChange(false);
                  }}
                >
                  <FileText className="h-4 w-4" />
                  {tpl.name}
                </CommandItem>
              ))}
            </CommandGroup>
          </>
        ) : null}

        <CommandSeparator />

        <CommandGroup heading="Tarefas agendadas">
          {tasks.map((task) => (
            <CommandItem
              key={task.id}
              value={`task tarefa ${task.name} ${task.command} ${task.schedule}`}
              onSelect={() => {
                onLaunchCommand({
                  title: task.name,
                  command: task.command,
                  cwdMode: task.cwdMode ?? "worktree",
                  description: task.description ?? null,
                });
                onOpenChange(false);
              }}
            >
              <Clock3 className="h-4 w-4" />
              {task.name}
            </CommandItem>
          ))}
        </CommandGroup>

        {activeCombId || activePaneId ? (
          <>
            <CommandSeparator />
          <CommandGroup heading="Foco atual">
            {activeCombId ? (
                <CommandItem
                  value="workspace ativo"
                  onSelect={() => {
                    onSelectWorkspace(activeCombId);
                    onOpenChange(false);
                  }}
                >
                  <PanelLeftOpen className="h-4 w-4" />
                  Workspace ativo
                </CommandItem>
              ) : null}
              {activePaneId ? (
                <CommandItem
                  value="pane ativo"
                  onSelect={() => {
                    onSelectPane(activePaneId);
                    onOpenChange(false);
                  }}
                >
                  <Terminal className="h-4 w-4" />
                  Pane ativo
                </CommandItem>
              ) : null}
            </CommandGroup>
          </>
        ) : null}
      </CommandList>
    </CommandDialog>
  );
}
