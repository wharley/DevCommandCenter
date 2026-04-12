"use client";

import { useMemo } from "react";
import {
  Bot,
  FolderGit2,
  ListPlus,
  PanelLeftOpen,
  Settings2,
  Terminal,
  Workflow,
  WandSparkles,
  Clock3,
  FileText,
} from "lucide-react";
import { CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandSeparator } from "@/components/ui/command";
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
  onOpenRepoConfig: () => void;
  onSelectProject: (projectId: string) => void;
  onSelectWorkspace: (combId: string) => void;
  onSelectPane: (paneId: string) => void;
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
  onOpenRepoConfig,
  onSelectProject,
  onSelectWorkspace,
  onSelectPane,
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
          </CommandItem>
          <CommandItem
            value="configurações"
            onSelect={() => {
              onOpenSettings();
              onOpenChange(false);
            }}
          >
            <Settings2 className="h-4 w-4" />
            Abrir settings
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
