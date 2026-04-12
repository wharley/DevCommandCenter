"use client";

import React, { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import {
  Bell,
  Bot,
  Clock3,
  ChevronRight,
  ChevronDown,
  Database,
  FolderGit2,
  GitBranch,
  GitPullRequest,
  Loader2,
  Merge,
  Pin,
  Plus,
  RefreshCw,
  Settings,
  Terminal,
  Trash2,
  WandSparkles,
  Workflow,
} from "lucide-react";
import { toast } from "sonner";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { EmbeddedTerminal } from "@/components/embedded-terminal";
import { AddProjectDialog } from "@/components/dialogs/add-project-dialog";
import { ProjectRepoConfigDialog } from "@/components/dialogs/project-repo-config-dialog";
import { ProjectRepoTomlDialog } from "@/components/dialogs/project-repo-toml-dialog";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import { usePanes, useProjects, useProviders } from "@/hooks/use-data";
import SettingsPage from "@/src/pages/SettingsPage";
import { normalizeComb, normalizeCombs, normalizePanes } from "@/lib/database/normalize";
import type {
  Comb,
  CreateCombDTO,
  Pane,
  Project,
  ProjectRepoConfig,
  RepoTaskDefinition,
  RepoTaskTemplate,
  Provider,
} from "@/lib/database/types";
import type { DaemonDiffBundleItem, DaemonStatus, DaemonTaskStatus } from "@/types/app";
import {
  useTerminalAttentionToasts,
  type TerminalAttentionRecord,
} from "@/hooks/use-terminal-attention-toasts";
import {
  bumpTerminalFontSize,
  resetTerminalFontSize,
} from "@/lib/terminal/terminal-preferences";
import { useTerminalProjectActivity } from "@/hooks/use-terminal-project-activity";
import { WorkspaceCommandPalette } from "@/components/workspace-command-palette";
import { ProcessesPanel } from "@/components/processes-panel";
import { getCombDiscardDialogCopy } from "@/lib/comb-discard-confirmation";
import { useAppWindowTitle } from "@/hooks/use-app-window-title";

const CLI_PROVIDER_TYPES = ["codex", "claude-code", "gemini", "cursor"] as const;

const activePaneStorageKey = (combId: string) => `dcc:workspace:${combId}:activePane`;

function isCliProviderType(type: string): type is (typeof CLI_PROVIDER_TYPES)[number] {
  return CLI_PROVIDER_TYPES.includes(type as (typeof CLI_PROVIDER_TYPES)[number]);
}

function buildCliCommand(provider: Provider | null): string | undefined {
  if (!provider) return undefined;
  const t = provider.type;
  const cliPath = provider.cliPath?.trim();
  const usePath = cliPath && (cliPath.startsWith("/") || /^[A-Za-z]:\\/.test(cliPath));
  if (t === "codex") return usePath ? cliPath : "codex";
  if (t === "claude-code") return usePath ? cliPath : "claude";
  if (t === "gemini") return usePath ? cliPath : "gemini";
  if (t === "cursor") return usePath ? cliPath : "cursor-agent";
  return undefined;
}

/** Rótulo curto para badge: qual CLI/agent está ligado a este pane. */
function cliAgentKindLabel(provider: Provider | null): string {
  const t = provider?.type;
  if (t === "codex") return "Codex";
  if (t === "claude-code") return "Claude";
  if (t === "gemini") return "Gemini";
  if (t === "cursor") return "Cursor";
  return "Agent";
}

function getPaneRuntimeCommand(pane: Pane, provider: Provider | null): string | undefined {
  if (pane.type === "agent") return buildCliCommand(provider);
  return pane.initialPrompt?.trim() || undefined;
}

function getProjectConfigBranchPrefix(config: ProjectRepoConfig | null | undefined): string {
  return config?.branchPrefix?.trim() || "dcc";
}

function AgentKindBadge({
  provider,
  compact,
  className,
}: {
  provider: Provider | null;
  compact?: boolean;
  className?: string;
}) {
  const label = cliAgentKindLabel(provider);
  const tip = provider?.name?.trim() ? provider.name : label;
  return (
    <Badge
      variant="secondary"
      title={tip}
      className={`shrink-0 font-normal ${compact ? "h-5 px-1.5 py-0 text-[10px] leading-none" : "text-xs"} ${className ?? ""}`}
    >
      {label}
    </Badge>
  );
}

function NewWorkspaceDialog({
  open,
  onOpenChange,
  projects,
  selectedProjectId,
  onCreate,
  createComb,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  projects: Project[];
  selectedProjectId: string | null;
  onCreate: (comb: Comb) => void;
  createComb: (data: CreateCombDTO) => Promise<Comb>;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [baseBranch, setBaseBranch] = useState("main");
  const [projectId, setProjectId] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [branchList, setBranchList] = useState<string[]>([]);
  const [branchesLoading, setBranchesLoading] = useState(false);

  const selectedProject = useMemo(
    () => projects.find((p) => p.id === projectId) ?? null,
    [projects, projectId],
  );

  useEffect(() => {
    if (!open) return;
    setName("");
    setDescription("");
    setBaseBranch("main");
    setProjectId(selectedProjectId ?? projects[0]?.id ?? "");
    setBranchList([]);
  }, [open, selectedProjectId, projects]);

  useEffect(() => {
    if (!open) return;
    const path = selectedProject?.path?.trim();
    if (!path) {
      setBranchList([]);
      setBranchesLoading(false);
      return;
    }
    const git = window.desktopAPI?.git;
    if (!git?.getLocalBranches) {
      setBranchList([]);
      setBranchesLoading(false);
      return;
    }
    let cancelled = false;
    setBranchesLoading(true);
    const currentP =
      git.getCurrentBranch?.(path) ?? Promise.resolve("");
    Promise.all([git.getLocalBranches(path), currentP])
      .then(([branches, current]) => {
        if (cancelled) return;
        const list = (branches ?? []).map((b) => b.trim()).filter(Boolean);
        setBranchList(list);
        const c = (current ?? "").trim();
        setBaseBranch((prev) => {
          if (list.includes(prev)) return prev;
          if (c && list.includes(c)) return c;
          return (list[0] ?? prev) || "main";
        });
      })
      .catch(() => {
        if (!cancelled) setBranchList([]);
      })
      .finally(() => {
        if (!cancelled) setBranchesLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, selectedProject?.path, projectId]);

  const handleCreate = async () => {
    if (!name.trim() || !projectId) {
      toast.error("Preencha projeto e nome do workspace.");
      return;
    }
    setIsCreating(true);
    try {
      const comb = await createComb({
        projectId,
        name: name.trim(),
        description: description.trim() || undefined,
        baseBranch: baseBranch.trim() || "main",
      });
      onCreate(comb);
      onOpenChange(false);
      toast.success("Workspace criado");
    } catch {
      toast.error("Falha ao criar workspace");
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Novo Workspace</DialogTitle>
          <DialogDescription>Cria uma sessão isolada para terminais e agents.</DialogDescription>
        </DialogHeader>
        <div className="rounded-md border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          <WorkflowTip />
        </div>
        <div className="space-y-3 py-2">
          <div className="space-y-2">
            <label className="text-sm font-medium">Projeto</label>
            <Select value={projectId} onValueChange={setProjectId}>
              <SelectTrigger>
                <SelectValue placeholder="Selecione o projeto" />
              </SelectTrigger>
              <SelectContent>
                {projects.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Nome</label>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="ex.: auth-refactor" />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Branch base</label>
            {branchesLoading ? (
              <div className="flex h-10 items-center gap-2 rounded-md border border-input bg-background px-3 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 shrink-0 animate-spin" />
                A carregar branches…
              </div>
            ) : branchList.length > 0 ? (
              <Select value={baseBranch} onValueChange={setBaseBranch}>
                <SelectTrigger>
                  <SelectValue placeholder="Selecione a branch" />
                </SelectTrigger>
                <SelectContent>
                  {branchList.map((br) => (
                    <SelectItem key={br} value={br}>
                      {br}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input
                value={baseBranch}
                onChange={(e) => setBaseBranch(e.target.value)}
                placeholder="main"
              />
            )}
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Descrição (opcional)</label>
            <Textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={2} />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancelar
          </Button>
          <Button onClick={handleCreate} disabled={isCreating}>
            {isCreating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Criar
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function NewAgentPaneDialog({
  open,
  onOpenChange,
  combId,
  providers,
  preferredProviderId,
  onCreate,
  ensureCombWorktree,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  combId: string;
  providers: Provider[];
  preferredProviderId?: string | null;
  onCreate: (pane: Pane) => void;
  ensureCombWorktree: () => Promise<boolean>;
}) {
  const [providerId, setProviderId] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const { create } = usePanes(combId);
  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProviderType(p.type)),
    [providers],
  );

  useEffect(() => {
    if (!open) return;
    if (preferredProviderId && cliProviders.some((p) => p.id === preferredProviderId)) {
      setProviderId(preferredProviderId);
      return;
    }
    setProviderId(cliProviders[0]?.id ?? "");
  }, [open, cliProviders, preferredProviderId]);

  const handleCreate = async () => {
    setIsCreating(true);
    try {
      const wtOk = await ensureCombWorktree();
      if (!wtOk) return;
      const pane = await create({
        combId,
        type: "agent",
        providerId: providerId || undefined,
      });
      onCreate(pane);
      onOpenChange(false);
      toast.success("Agent pane criado");
    } catch {
      toast.error("Falha ao criar pane");
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Novo Agent Pane</DialogTitle>
          <DialogDescription>Selecione o agente CLI para abrir no workspace.</DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <div className="space-y-2">
            <label className="text-sm font-medium">Agente CLI</label>
            <Select value={providerId} onValueChange={setProviderId}>
              <SelectTrigger>
                <SelectValue placeholder="Selecione agente..." />
              </SelectTrigger>
              <SelectContent>
                {cliProviders.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancelar
          </Button>
          <Button onClick={handleCreate} disabled={isCreating}>
            {isCreating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Criar Agent
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

const WorkspaceListItem = React.memo(function WorkspaceListItem({
  comb,
  isActive,
  projectName,
  attentionExcerpt,
  hasAttention,
  runningCount,
  onSelect,
  onSelectBegin,
  onRemove,
  onTogglePin,
}: {
  comb: Comb;
  isActive: boolean;
  projectName: string;
  attentionExcerpt: string | null;
  hasAttention: boolean;
  runningCount: number;
  onSelect: (comb: Comb) => void;
  onSelectBegin?: (comb: Comb) => void;
  onRemove: (combId: string) => void;
  onTogglePin: (combId: string) => void;
}) {
  const handleSelect = useCallback(() => onSelect(comb), [onSelect, comb]);
  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (e.button !== 0) return;
      onSelectBegin?.(comb);
    },
    [onSelectBegin, comb],
  );
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        onSelectBegin?.(comb);
        onSelect(comb);
      }
    },
    [onSelectBegin, onSelect, comb],
  );
  const handleRemove = useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      e.stopPropagation();
      onRemove(comb.id);
    },
    [onRemove, comb.id],
  );
  const handleTogglePin = useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      e.stopPropagation();
      onTogglePin(comb.id);
    },
    [onTogglePin, comb.id],
  );

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={handleSelect}
      onPointerDown={handlePointerDown}
      onKeyDown={handleKeyDown}
      className={`titlebar-no-drag group cursor-pointer rounded-lg border px-2.5 py-2 transition-colors ${
        isActive ? "border-primary bg-sidebar-accent text-sidebar-accent-foreground" : "border-transparent hover:bg-sidebar-accent/50"
      }`}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="truncate text-sm font-medium">{comb.name}</span>
            {hasAttention ? <span className="h-2 w-2 rounded-full bg-sky-400" /> : null}
          </div>
          <div className="mt-0.5 flex items-center gap-1 text-[11px] text-sidebar-foreground/60">
            <GitBranch className="h-3 w-3" />
            <span className="truncate">{comb.branch ?? comb.baseBranch}</span>
          </div>
          <p className="mt-0.5 line-clamp-1 text-[10px] text-sidebar-foreground/50">{projectName}</p>
          {runningCount > 0 ? (
            <Badge variant="outline" className="mt-1 h-5 border-sidebar-border px-1.5 text-[10px] text-sidebar-foreground/70">
              {runningCount} ativos
            </Badge>
          ) : null}
          {attentionExcerpt ? <p className="mt-1 line-clamp-1 text-[11px] text-sidebar-foreground/70">{attentionExcerpt}</p> : null}
        </div>
        <div className="flex items-center gap-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className={`h-6 w-6 ${comb.isPinned ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
                onPointerDown={(e) => e.stopPropagation()}
                onClick={handleTogglePin}
              >
                <Pin className={`h-3.5 w-3.5 ${comb.isPinned ? "fill-current text-primary" : ""}`} />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{comb.isPinned ? "Desafixar workspace" : "Fixar workspace"}</TooltipContent>
          </Tooltip>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6 opacity-0 group-hover:opacity-100"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={handleRemove}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
    </div>
  );
});

const PaneCard = React.memo(function PaneCard({
  pane,
  worktreePath,
  provider,
  onPaneStatusChange,
  onRemovePane,
}: {
  pane: Pane;
  worktreePath: string;
  provider: Provider | null;
  onPaneStatusChange: (paneId: string, status: "running" | "exited") => void;
  onRemovePane: (paneId: string) => void;
}) {
  const command = getPaneRuntimeCommand(pane, provider);
  const label = pane.type === "agent" ? (pane.title ?? provider?.name ?? "Agent") : (pane.title ?? "Terminal");
  const args = pane.type === "agent" && pane.initialPrompt ? [pane.initialPrompt] : [];
  const handleRemove = useCallback(() => onRemovePane(pane.id), [onRemovePane, pane.id]);
  /** Sincroniza badge com sessão PTY no backend (reattach ao mudar de pane). */
  const [agentStatus, setAgentStatus] = useState<"running" | "exited" | null>(null);

  const handleAgentExit = useCallback(() => {
    setAgentStatus("exited");
    onPaneStatusChange(pane.id, "exited");
  }, [onPaneStatusChange, pane.id]);

  const handleAgentSessionActive = useCallback(() => {
    setAgentStatus("running");
    onPaneStatusChange(pane.id, "running");
  }, [onPaneStatusChange, pane.id]);

  const isAgent = pane.type === "agent";

  useEffect(() => {
    if (!isAgent) return;
    const api = window.desktopAPI?.terminal;
    if (!api?.getPaneSession) return;
    let cancelled = false;
    void api.getPaneSession(pane.id).then((session) => {
      if (cancelled) return;
      if (!session || typeof session !== "object") {
        setAgentStatus(null);
        return;
      }
      const s = session as { ptyId?: string; status?: string };
      if (s.status === "running" && s.ptyId) {
        setAgentStatus("running");
        onPaneStatusChange(pane.id, "running");
      } else if (s.status === "exited") {
        setAgentStatus("exited");
        onPaneStatusChange(pane.id, "exited");
      } else {
        setAgentStatus(null);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [isAgent, pane.id, onPaneStatusChange]);

  const isManagedProcess = pane.type === "term" && !!pane.initialPrompt?.trim();

  return (
    <div data-pane-id={pane.id} className="flex h-full min-h-0 flex-col overflow-hidden">
      <div className="mb-1 flex items-center justify-between gap-2 rounded border border-border px-2 py-1">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          {isAgent ? <AgentKindBadge provider={provider} /> : null}
          <span className="truncate text-xs">{label}</span>
          {(isAgent || isManagedProcess) && agentStatus === "running" ? (
            <Badge variant="default" className="h-5 px-1.5 py-0 text-[10px]">
              Rodando
            </Badge>
          ) : null}
          {(isAgent || isManagedProcess) && agentStatus === "exited" ? (
            <Badge variant="outline" className="h-5 px-1.5 py-0 text-[10px]">
              Finalizado
            </Badge>
          ) : null}
        </div>
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="icon" className="h-6 w-6 shrink-0" onClick={handleRemove}>
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden">
        {isAgent && !command ? (
          <p className="rounded border border-amber-500/40 bg-amber-500/10 px-2 py-2 text-xs text-amber-200">
            Nenhum CLI resolvido para este provedor. Configura o caminho do executável em Providers ou escolhe um
            provedor ativo (Codex, Claude, Cursor, etc.).
          </p>
        ) : null}
        <EmbeddedTerminal
          cwd={worktreePath}
          command={command}
          args={args}
          paneId={pane.id}
          title={label}
          onSessionActive={isAgent ? handleAgentSessionActive : undefined}
          onExit={handleAgentExit}
        />
      </div>
    </div>
  );
});

function WorkflowTip({ centered = false }: { centered?: boolean }) {
  return (
    <div className={`flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground ${centered ? "justify-center" : ""}`}>
      <FolderGit2 className="h-3.5 w-3.5" />
      <span>Projeto</span>
      <ChevronRight className="h-3.5 w-3.5 opacity-60" />
      <GitBranch className="h-3.5 w-3.5" />
      <span>Workspace</span>
      <ChevronRight className="h-3.5 w-3.5 opacity-60" />
      <Terminal className="h-3.5 w-3.5" />
      <span>Panes</span>
      <ChevronRight className="h-3.5 w-3.5 opacity-60" />
      <GitPullRequest className="h-3.5 w-3.5" />
      <span>Git/PR</span>
      <ChevronRight className="h-3.5 w-3.5 opacity-60" />
      <Merge className="h-3.5 w-3.5" />
      <span>Merge</span>
    </div>
  );
}

function SidebarSection({
  title,
  description,
  defaultOpen = false,
  count,
  children,
}: {
  title: string;
  description?: string;
  defaultOpen?: boolean;
  count?: string;
  children: React.ReactNode;
}) {
  return (
    <Collapsible defaultOpen={defaultOpen} className="rounded-lg border border-sidebar-border/70 bg-sidebar-accent/20">
      <CollapsibleTrigger className="group flex w-full items-center justify-between gap-3 px-3 py-2 text-left transition-colors hover:bg-sidebar-accent/35">
        <div className="min-w-0">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-sidebar-foreground/60">
            {title}
          </p>
          {description ? (
            <p className="mt-0.5 text-[11px] text-sidebar-foreground/45">
              {description}
            </p>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          {count ? (
            <Badge variant="outline" className="border-sidebar-border text-[10px] text-sidebar-foreground/70">
              {count}
            </Badge>
          ) : null}
          <ChevronDown className="h-4 w-4 shrink-0 opacity-70 transition-transform duration-200 group-data-[state=open]:rotate-180" />
        </div>
      </CollapsibleTrigger>
      <CollapsibleContent className="border-t border-sidebar-border/60 px-3 pb-3 pt-2">
        {children}
      </CollapsibleContent>
    </Collapsible>
  );
}

export default function CmuxWorkspacePage() {
  const { projects, isLoading: projectsLoading, update: updateProject, refresh: refreshProjects } = useProjects();
  const { providers } = useProviders();
  const { confirmDialog } = useConfirmDialog();

  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [activeCombId, setActiveCombId] = useState<string | null>(null);
  const [showProviders, setShowProviders] = useState(false);
  const [newCombOpen, setNewCombOpen] = useState(false);
  const [newAgentOpen, setNewAgentOpen] = useState(false);
  const [addProjectOpen, setAddProjectOpen] = useState(false);
  const [repoConfigOpen, setRepoConfigOpen] = useState(false);
  const [repoConfigTomlOpen, setRepoConfigTomlOpen] = useState(false);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [attentionOpen, setAttentionOpen] = useState(false);
  const [daemonStatus, setDaemonStatus] = useState<DaemonStatus | null>(null);
  const [daemonTasks, setDaemonTasks] = useState<DaemonTaskStatus[]>([]);
  const [taskTemplates, setTaskTemplates] = useState<RepoTaskTemplate[]>([]);
  const [daemonCombs, setDaemonCombs] = useState<Comb[]>([]);
  const [daemonPanes, setDaemonPanes] = useState<Pane[]>([]);
  const [daemonDiffBundle, setDaemonDiffBundle] = useState<DaemonDiffBundleItem[]>([]);
  const [daemonExplorerLoading, setDaemonExplorerLoading] = useState(false);
  const [activePaneId, setActivePaneId] = useState<string | null>(null);
  const [attentionRecords, setAttentionRecords] = useState<TerminalAttentionRecord[]>([]);
  const [initializingBasePaneIds, setInitializingBasePaneIds] = useState<Set<string>>(new Set());
  /** Feedback imediato na sidebar antes do commit pesado (xterm / área principal). */
  const [pointerSelectedCombId, setPointerSelectedCombId] = useState<string | null>(null);
  const pointerPressClearTimeoutRef = useRef<number | null>(null);
  const hydratedAttentionRef = useRef(false);

  const sortedProjects = useMemo(
    () => [...projects].sort((a, b) => +new Date(b.updatedAt) - +new Date(a.updatedAt)),
    [projects],
  );
  const [combs, setCombs] = useState<Comb[]>([]);
  const [combsLoading, setCombsLoading] = useState(true);
  const refreshCombs = useCallback(async () => {
    if (!window.db?.combs || sortedProjects.length === 0) {
      setCombs([]);
      setCombsLoading(false);
      return;
    }
    setCombsLoading(true);
    try {
      const chunks = await Promise.all(
        sortedProjects.map((project) => window.db!.combs.findByProject(project.id)),
      );
      const flat = normalizeCombs(chunks.flat()) as Comb[];
      // Ordenar: fixados primeiro (por pinnedAt desc), depois não-fixados (por updatedAt desc)
      flat.sort((a, b) => {
        if (a.isPinned && !b.isPinned) return -1;
        if (!a.isPinned && b.isPinned) return 1;
        if (a.isPinned && b.isPinned) {
          return +new Date(b.pinnedAt ?? 0) - +new Date(a.pinnedAt ?? 0);
        }
        return +new Date(b.updatedAt) - +new Date(a.updatedAt);
      });
      setCombs(flat);
    } finally {
      setCombsLoading(false);
    }
  }, [sortedProjects]);

  const createComb = useCallback(async (data: CreateCombDTO) => {
    if (!window.db?.combs) throw new Error("Combs indisponivel");
    const created = await window.db.combs.create(data);
    const comb = normalizeComb(created as unknown as Record<string, unknown>) as unknown as Comb;
    await refreshCombs();
    return comb;
  }, [refreshCombs]);
  const combsRef = useRef(combs);
  combsRef.current = combs;
  const activeComb = useMemo(
    () => (activeCombId ? (combs.find((c) => c.id === activeCombId) ?? null) : null),
    [activeCombId, combs],
  );
  const activeProject = useMemo(() => {
    if (!activeComb) return null;
    return sortedProjects.find((project) => project.id === activeComb.projectId) ?? null;
  }, [activeComb, sortedProjects]);
  const windowTitleProject = useMemo(
    () =>
      activeProject ??
      (selectedProjectId
        ? (sortedProjects.find((p) => p.id === selectedProjectId) ?? null)
        : null),
    [activeProject, selectedProjectId, sortedProjects],
  );
  useAppWindowTitle(windowTitleProject, activeComb);
  const { activity: projectActivity } = useTerminalProjectActivity(activeProject?.id ?? null);
  const {
    panes,
    isLoading: panesLoading,
    refresh: refreshPanes,
    create: createPane,
    update: updatePane,
    remove: removePane,
  } = usePanes(activeCombId ?? undefined);
  useEffect(() => {
    void refreshCombs();
  }, [refreshCombs]);

  const providerById = useMemo(() => {
    const map = new Map<string, Provider>();
    for (const p of providers) map.set(p.id, p);
    return map;
  }, [providers]);
  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProviderType(p.type)),
    [providers],
  );
  const activeRepoConfig = activeProject?.repoConfig ?? null;
  const activeRepoTasks = useMemo(
    () => activeRepoConfig?.tasks ?? [],
    [activeRepoConfig],
  );
  useEffect(() => {
    const path = activeProject?.path?.trim();
    if (!path || !window.desktopAPI?.repo?.listTaskTemplates) {
      setTaskTemplates([]);
      return;
    }
    let cancelled = false;
    void window.desktopAPI.repo
      .listTaskTemplates(path)
      .then((list) => {
        if (!cancelled) setTaskTemplates(Array.isArray(list) ? list : []);
      })
      .catch(() => {
        if (!cancelled) setTaskTemplates([]);
      });
    return () => {
      cancelled = true;
    };
  }, [activeProject?.path]);
  const daemonApi = window.desktopAPI?.daemon;
  const projectNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const p of sortedProjects) map.set(p.id, p.name);
    return map;
  }, [sortedProjects]);
  const combById = useMemo(() => {
    const map = new Map<string, Comb>();
    for (const comb of combs) map.set(comb.id, comb);
    return map;
  }, [combs]);
  const unreadAttentionByCombId = useMemo(() => {
    const map = new Map<string, TerminalAttentionRecord>();
    for (const record of attentionRecords) {
      if (!record.read && !map.has(record.combId)) {
        map.set(record.combId, record);
      }
    }
    return map;
  }, [attentionRecords]);
  const hasUnreadAttentionByPaneId = useMemo(() => {
    const set = new Set<string>();
    for (const record of attentionRecords) {
      if (!record.read) set.add(record.paneId);
    }
    return set;
  }, [attentionRecords]);
  const unreadCount = useMemo(() => {
    let count = 0;
    for (const r of attentionRecords) {
      if (!r.read) count += 1;
    }
    return count;
  }, [attentionRecords]);
  const daemonTaskById = useMemo(() => {
    const map = new Map<string, DaemonTaskStatus>();
    for (const task of daemonTasks) {
      if (activeProject?.id && task.projectId !== activeProject.id) continue;
      map.set(task.taskId, task);
    }
    return map;
  }, [activeProject?.id, daemonTasks]);

  const isAttentionPaneInView = useCallback(
    (detail: { paneId: string; combId: string }) => {
      if (showProviders) return false;
      if (!activeCombId || !activePaneId) return false;
      return detail.combId === activeCombId && detail.paneId === activePaneId;
    },
    [showProviders, activeCombId, activePaneId],
  );

  useTerminalAttentionToasts({
    onNavigateToPane: ({ projectId, combId, paneId }) => {
      setSelectedProjectId(projectId);
      setActiveCombId(combId);
      setActivePaneId(paneId);
    },
    onAttentionRecord: (record) => {
      setAttentionRecords((prev) => {
        if (prev.length > 0 && prev[0].paneId === record.paneId && prev[0].excerpt === record.excerpt) {
          return prev;
        }
        const next = [record, ...prev].slice(0, 120);
        return next;
      });
    },
    onAttentionAction: (event) => {
      setAttentionRecords((prev) =>
        prev.map((record) =>
          record.id === event.notificationId ||
          (record.paneId === event.paneId && record.combId === event.combId)
            ? { ...record, read: true }
            : record,
        ),
      );
      if (event.actionId === "dismiss" || event.actionId === "__closed") {
        return;
      }
      if (event.actionId === "reply") {
        setAttentionOpen(false);
      }
    },
    isAttentionPaneInView,
  });

  useEffect(() => {
    if (hydratedAttentionRef.current) return;
    hydratedAttentionRef.current = true;
    const raw = localStorage.getItem("dcc:attention:records");
    if (!raw) return;
    try {
      const parsed = JSON.parse(raw) as TerminalAttentionRecord[];
      if (Array.isArray(parsed))
        setAttentionRecords(
          parsed.slice(0, 120).map((r) => ({
            ...r,
            phase: r.phase ?? "needs_input",
          })),
        );
    } catch {
      // ignore malformed storage
    }
  }, []);

  useEffect(() => {
    localStorage.setItem("dcc:attention:records", JSON.stringify(attentionRecords.slice(0, 120)));
  }, [attentionRecords]);

  useEffect(() => {
    if (!daemonApi?.getStatus || !daemonApi?.listTasks) return;
    let cancelled = false;
    const refreshDaemon = async () => {
      try {
        const [status, tasks] = await Promise.all([
          daemonApi.getStatus(),
          daemonApi.listTasks(),
        ]);
        if (cancelled) return;
        setDaemonStatus(status);
        setDaemonTasks(tasks ?? []);
      } catch {
        if (!cancelled) {
          setDaemonStatus(null);
          setDaemonTasks([]);
        }
      }
    };
    void refreshDaemon();
    const timer = window.setInterval(() => {
      void refreshDaemon();
    }, 5000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [daemonApi]);

  const refreshDaemonExplorer = useCallback(async () => {
    if (!daemonApi?.listCombs || !daemonApi?.listPanes || !daemonApi?.getDiffsBundle) {
      setDaemonCombs([]);
      setDaemonPanes([]);
      setDaemonDiffBundle([]);
      setDaemonExplorerLoading(false);
      return;
    }

    setDaemonExplorerLoading(true);
    try {
      const projectId = activeProject?.id ?? undefined;
      const combId = activeCombId ?? undefined;
      const [combsRaw, panesRaw] = await Promise.all([
        daemonApi.listCombs(projectId),
        daemonApi.listPanes(projectId, combId),
      ]);
      const normalizedDaemonCombs = normalizeCombs((combsRaw ?? []) as unknown[]) as Comb[];
      const normalizedDaemonPanes = normalizePanes((panesRaw ?? []) as unknown[]) as Pane[];
      setDaemonCombs(normalizedDaemonCombs);
      setDaemonPanes(normalizedDaemonPanes);

      const worktreePaths = normalizedDaemonCombs
        .map((comb) => comb.worktreePath?.trim())
        .filter((value): value is string => Boolean(value));
      if (worktreePaths.length === 0) {
        setDaemonDiffBundle([]);
        return;
      }

      const bundleRaw = await daemonApi.getDiffsBundle(
        worktreePaths,
        normalizedDaemonCombs.map((comb) => comb.id),
      );
      setDaemonDiffBundle(Array.isArray(bundleRaw) ? (bundleRaw as DaemonDiffBundleItem[]) : []);
    } catch {
      setDaemonCombs([]);
      setDaemonPanes([]);
      setDaemonDiffBundle([]);
    } finally {
      setDaemonExplorerLoading(false);
    }
  }, [activeCombId, activeProject?.id, daemonApi]);

  useEffect(() => {
    void refreshDaemonExplorer();
    const timer = window.setInterval(() => {
      void refreshDaemonExplorer();
    }, 15000);
    return () => {
      window.clearInterval(timer);
    };
  }, [refreshDaemonExplorer]);

  useEffect(() => {
    if (selectedProjectId && projects.some((p) => p.id === selectedProjectId)) return;
    const stored = localStorage.getItem("dcc:workspace:selectedProjectId");
    if (stored && projects.some((p) => p.id === stored)) {
      setSelectedProjectId(stored);
      return;
    }
    if (projects.length > 0) setSelectedProjectId(sortedProjects[0].id);
  }, [projects, selectedProjectId, sortedProjects]);

  useEffect(() => {
    if (selectedProjectId) localStorage.setItem("dcc:workspace:selectedProjectId", selectedProjectId);
  }, [selectedProjectId]);

  useEffect(() => {
    if (!selectedProjectId) {
      setActiveCombId(null);
      return;
    }
    if (activeCombId && combs.some((c) => c.id === activeCombId)) return;
    const stored = localStorage.getItem(`dcc:workspace:${selectedProjectId}:activeComb`);
    if (stored && combs.some((c) => c.id === stored)) {
      setActiveCombId(stored);
      return;
    }
    const firstProjectComb = combs.find((c) => c.projectId === selectedProjectId);
    setActiveCombId(firstProjectComb?.id ?? null);
  }, [combs, activeCombId, selectedProjectId]);

  useEffect(() => {
    if (activeCombId && selectedProjectId) {
      localStorage.setItem(`dcc:workspace:${selectedProjectId}:activeComb`, activeCombId);
      setAttentionRecords((prev) => {
        let changed = false;
        const next = prev.map((item) => {
          if (item.combId !== activeCombId || item.read) return item;
          changed = true;
          return { ...item, read: true };
        });
        return changed ? next : prev;
      });
    }
  }, [activeCombId, selectedProjectId]);

  const visiblePanes = useMemo(
    () => panes.filter((pane) => !initializingBasePaneIds.has(pane.id)),
    [panes, initializingBasePaneIds],
  );
  const activePane = useMemo(
    () => (activePaneId ? (visiblePanes.find((pane) => pane.id === activePaneId) ?? null) : null),
    [activePaneId, visiblePanes],
  );
  useEffect(() => {
    if (!activeCombId) return;
    if (!activePaneId || !visiblePanes.some((p) => p.id === activePaneId)) return;
    localStorage.setItem(activePaneStorageKey(activeCombId), activePaneId);
  }, [activeCombId, activePaneId, visiblePanes]);

  useEffect(() => {
    if (visiblePanes.length === 0) {
      if (activePaneId !== null) setActivePaneId(null);
      return;
    }
    if (activePaneId && visiblePanes.some((pane) => pane.id === activePaneId)) return;
    const stored =
      activeCombId && typeof window !== "undefined"
        ? localStorage.getItem(activePaneStorageKey(activeCombId))
        : null;
    if (stored && visiblePanes.some((pane) => pane.id === stored)) {
      setActivePaneId(stored);
      return;
    }
    setActivePaneId(visiblePanes[0].id);
  }, [visiblePanes, activePaneId, activeCombId]);

  const markPaneAttentionAsRead = useCallback((paneId: string) => {
    setAttentionRecords((prev) => {
      let changed = false;
      const next = prev.map((record) => {
        if (record.paneId !== paneId || record.read) return record;
        changed = true;
        return { ...record, read: true };
      });
      return changed ? next : prev;
    });
  }, []);
  const handleSelectPaneTab = useCallback((paneId: string) => {
    setActivePaneId(paneId);
    markPaneAttentionAsRead(paneId);
  }, [markPaneAttentionAsRead]);

  /**
   * Atalhos estilo Maestro: Cmd+1–9 (foco), Cmd+K (limpar), zoom, Shift+Cmd+[ ] (ciclo).
   * Ignora quando Providers está aberto ou foco em dialog/input (exceto textarea do xterm).
   * Conflitos possíveis: Cmd+K noutras apps; zoom do browser — aqui preventDefault no workspace.
   */
  useEffect(() => {
    const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/i.test(navigator.platform);
    const mod = isMac ? (e: KeyboardEvent) => e.metaKey : (e: KeyboardEvent) => e.ctrlKey;

    const onKeyDown = (event: KeyboardEvent) => {
      if (showProviders) return;

      if (mod(event) && (event.key === "k" || event.key === "K")) {
        event.preventDefault();
        if (event.shiftKey) {
          window.dispatchEvent(new CustomEvent("dcc-terminal-action", { detail: { type: "clearScrollback" } }));
        } else {
          setCommandPaletteOpen(true);
        }
        return;
      }

      if (!activeCombId) return;

      const el = event.target;
      if (el instanceof HTMLElement) {
        const inXterm = el.closest(".xterm");
        const inDialog = el.closest("[role=\"dialog\"], [data-radix-dialog-content]");
        if (inDialog && !inXterm) return;
        if (el.closest("input, textarea, select") && !inXterm) return;
      }

      if (!mod(event)) return;

      if (event.shiftKey && visiblePanes.length > 1) {
        const key = event.key;
        if (key === "]" || key === "[") {
          event.preventDefault();
          const currentIdx = visiblePanes.findIndex((pane) => pane.id === activePaneId);
          const baseIdx = currentIdx >= 0 ? currentIdx : 0;
          const nextIdx =
            key === "]"
              ? (baseIdx + 1) % visiblePanes.length
              : (baseIdx - 1 + visiblePanes.length) % visiblePanes.length;
          const nextPaneId = visiblePanes[nextIdx]?.id;
          if (!nextPaneId) return;
          setActivePaneId(nextPaneId);
          markPaneAttentionAsRead(nextPaneId);
          return;
        }
      }

      if (/^[1-9]$/.test(event.key) && visiblePanes.length > 0) {
        event.preventDefault();
        const idx = Number(event.key) - 1;
        const pane = visiblePanes[idx];
        if (pane) {
          setActivePaneId(pane.id);
          markPaneAttentionAsRead(pane.id);
        }
        return;
      }
      if (event.key === "=" || event.key === "+") {
        event.preventDefault();
        bumpTerminalFontSize(1);
        return;
      }
      if (event.key === "-" || event.key === "_") {
        event.preventDefault();
        bumpTerminalFontSize(-1);
        return;
      }
      if (event.key === "0") {
        event.preventDefault();
        resetTerminalFontSize();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    activeCombId,
    activePaneId,
    markPaneAttentionAsRead,
    showProviders,
    visiblePanes,
  ]);

  const ensureActiveCombWorktree = useCallback(async (): Promise<boolean> => {
    if (!activeCombId) return false;
    const comb = combsRef.current.find((c) => c.id === activeCombId);
    if (!comb) return false;
    if (comb.worktreePath) return true;
    const api = window.desktopAPI?.comb?.ensureWorktree;
    if (!api) return false;
    try {
      const result = await api(activeCombId);
      if (result.success) {
        await refreshCombs();
        return true;
      }
      if (result.error) toast.error(`Worktree: ${result.error}`);
      return false;
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : "Falha ao preparar worktree");
      return false;
    }
  }, [activeCombId, refreshCombs]);
  const handleSelectWorkspace = useCallback((comb: Comb) => {
    setSelectedProjectId(comb.projectId);
    setActiveCombId(comb.id);
    setShowProviders(false);
  }, []);

  const handleWorkspacePointerDown = useCallback((comb: Comb) => {
    if (pointerPressClearTimeoutRef.current != null) {
      window.clearTimeout(pointerPressClearTimeoutRef.current);
      pointerPressClearTimeoutRef.current = null;
    }
    flushSync(() => {
      setPointerSelectedCombId(comb.id);
    });
    pointerPressClearTimeoutRef.current = window.setTimeout(() => {
      pointerPressClearTimeoutRef.current = null;
      setPointerSelectedCombId((prev) => (prev === comb.id ? null : prev));
    }, 600);
  }, []);

  useLayoutEffect(() => {
    if (pointerSelectedCombId === null || pointerSelectedCombId !== activeCombId) return;
    if (pointerPressClearTimeoutRef.current != null) {
      window.clearTimeout(pointerPressClearTimeoutRef.current);
      pointerPressClearTimeoutRef.current = null;
    }
    setPointerSelectedCombId(null);
  }, [activeCombId, pointerSelectedCombId]);

  const handleAddTerminal = async () => {
    if (!activeCombId) return;
    try {
      const ok = await ensureActiveCombWorktree();
      if (!ok) return;
      const pane = await createPane({ combId: activeCombId, type: "term" });
      await refreshPanes();
      setActivePaneId(pane.id);
    } catch {
      toast.error("Falha ao abrir terminal");
    }
  };

  const launchManagedCommand = useCallback(
    async (payload: {
      title: string;
      command: string;
      cwdMode?: "project" | "worktree";
      description?: string | null;
    }) => {
      if (!activeCombId) return;
      const cwdMode = payload.cwdMode ?? "worktree";
      const projectPath = activeProject?.path?.trim() ?? "";
      let cwd = projectPath;
      if (cwdMode === "worktree") {
        const ok = await ensureActiveCombWorktree();
        if (!ok) return;
        const comb = window.db?.combs?.findById
          ? await window.db.combs.findById(activeCombId)
          : combsRef.current.find((item) => item.id === activeCombId) ?? null;
        cwd = (comb?.worktreePath ?? "").trim();
        if (!cwd) {
          toast.error("Worktree indisponível para este workspace.");
          return;
        }
      } else if (!projectPath) {
        toast.error("Caminho do projeto indisponível.");
        return;
      }

      try {
        const pane = await createPane({
          combId: activeCombId,
          type: "term",
          title: payload.title,
          initialPrompt: payload.command,
        });
        await updatePane(pane.id, {
          cwd,
          status: "running",
          lastActivityAt: new Date(),
        });
        await refreshPanes();
        setActivePaneId(pane.id);
        toast.success(payload.description ? `${payload.title} iniciado` : `Executando ${payload.title}`);
      } catch (error) {
        toast.error(error instanceof Error ? error.message : `Falha ao iniciar ${payload.title}`);
      }
    },
    [activeCombId, activeProject?.path, createPane, ensureActiveCombWorktree, refreshPanes, updatePane],
  );

  const runRepoTask = useCallback(
    async (task: RepoTaskDefinition) => {
      if (!activeProject) return;
      if (daemonApi?.runTask) {
        try {
          const result = await daemonApi.runTask(activeProject.id, task.id);
          if (!result.success) {
            toast.error(result.error ?? `Falha ao iniciar ${task.name}`);
            return;
          }
          setDaemonTasks((prev) => {
            if (!result.task) return prev;
            const next = prev.filter((item) => item.taskId !== task.id);
            next.push(result.task);
            return next;
          });
          toast.success(`${task.name} iniciado pelo daemon`);
          return;
        } catch (error) {
          toast.error(error instanceof Error ? error.message : `Falha ao iniciar ${task.name}`);
          return;
        }
      }

      await launchManagedCommand({
        title: task.name,
        command: task.command,
        cwdMode: task.cwdMode ?? "worktree",
        description: task.description ?? null,
      });
    },
    [activeProject, daemonApi, launchManagedCommand],
  );

  const handleOpenBaseTerminal = async () => {
    if (!activeCombId) return;
    const basePath = activeProject?.path?.trim();
    if (!basePath) {
      toast.error("Projeto base indisponivel para este workspace.");
      return;
      }
      try {
        const ok = await ensureActiveCombWorktree();
        if (!ok) return;
        const pane = await createPane({
        combId: activeCombId,
        type: "term",
        title: "Base",
      });
      setInitializingBasePaneIds((prev) => {
        const next = new Set(prev);
        next.add(pane.id);
        return next;
      });
      await updatePane(pane.id, { cwd: basePath });
      try {
        await window.desktopAPI?.terminal?.killByPaneId?.(pane.id);
      } catch {
        // best effort: if no previous PTY exists, ignore
      }
      await refreshPanes();
      setActiveCombId(activeCombId);
      setActivePaneId(pane.id);
      setInitializingBasePaneIds((prev) => {
        const next = new Set(prev);
        next.delete(pane.id);
        return next;
      });
    } catch (e: unknown) {
      setInitializingBasePaneIds(new Set());
      toast.error(
        e instanceof Error ? e.message : "Falha ao abrir terminal base no workspace.",
      );
    }
  };

  const handleRemovePane = async (paneId: string) => {
    const confirmed = await confirmDialog({
      title: "Remover pane?",
      description: "O terminal/agent será fechado.",
      confirmLabel: "Remover",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    const nextVisiblePanes = visiblePanes.filter((pane) => pane.id !== paneId);
    if (activePaneId === paneId) {
      const currentIdx = visiblePanes.findIndex((pane) => pane.id === paneId);
      const fallback =
        nextVisiblePanes[currentIdx] ??
        nextVisiblePanes[currentIdx - 1] ??
        nextVisiblePanes[0] ??
        null;
      setActivePaneId(fallback?.id ?? null);
    }
    try {
      await window.desktopAPI?.terminal?.killByPaneId?.(paneId);
    } catch {
      /* ignore */
    }
    await removePane(paneId);
    setAttentionRecords((prev) => prev.filter((item) => item.paneId !== paneId));
  };

  const handleRemoveWorkspace = async (combId: string) => {
    const dialogCopy = await getCombDiscardDialogCopy(combId);

    const confirmed = await confirmDialog({
      title: dialogCopy.title,
      description: dialogCopy.description,
      confirmLabel: dialogCopy.confirmLabel,
      cancelLabel: "Cancelar",
      confirmVariant: dialogCopy.confirmVariant,
    });

    if (!confirmed) return;

    try {
      const panesForComb = window.db?.panes?.findByComb
        ? await window.db.panes.findByComb(combId)
        : [];
      for (const pane of panesForComb ?? []) {
        if (!pane?.id) continue;
        try {
          await window.desktopAPI?.terminal?.killByPaneId?.(pane.id);
        } catch {
          /* ignore */
        }
      }
    } catch {
      /* ignore pane lookup errors */
    }

    if (window.desktopAPI?.comb?.discard) {
      const result = await window.desktopAPI.comb.discard(combId);
      if (!result.success && result.error) toast.error(result.error);
    }
    if (window.db?.combs) await window.db.combs.delete(combId);
    try {
      localStorage.removeItem(activePaneStorageKey(combId));
    } catch {
      /* ignore */
    }
    if (activeCombId === combId) setActiveCombId(null);
    refreshCombs();
    setAttentionRecords((prev) => prev.filter((item) => item.combId !== combId));
    toast.success("Workspace removido");
  };
  const handleRemoveWorkspaceById = useCallback((combId: string) => {
    void handleRemoveWorkspace(combId);
  }, [handleRemoveWorkspace]);
  const handleRemovePaneById = useCallback((paneId: string) => {
    void handleRemovePane(paneId);
  }, [handleRemovePane]);

  const handlePaneStatusChange = useCallback(
    async (paneId: string, status: "running" | "exited") => {
      try {
        await updatePane(paneId, {
          status,
          lastActivityAt: new Date(),
        });
      } catch {
        /* ignore status sync errors */
      }
    },
    [updatePane],
  );

  const handleTogglePin = useCallback(async (combId: string) => {
    if (!window.db?.combs) return;
    try {
      await window.db.combs.togglePin(combId);
      await refreshCombs();
      toast.success("Workspace atualizado");
    } catch (error) {
      toast.error("Falha ao atualizar workspace");
      console.error("Error toggling pin:", error);
    }
  }, [refreshCombs]);

  if (projectsLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-background">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="flex h-screen overflow-hidden bg-background">
      <div className="titlebar-drag-region fixed top-0 left-0 right-0 h-8 z-50" />
      <aside className="flex h-full min-h-0 w-72 shrink-0 flex-col overflow-hidden border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
        <div className="shrink-0 border-b border-sidebar-border px-3 pt-10 pb-3">
          <div className="mb-3 flex items-center justify-between">
            <div className="flex items-center gap-2">
              <img src="/dcc-mark.svg" alt="DCC" className="h-7 w-7 rounded-lg" />
              <span className="text-sm font-semibold tracking-tight">Dev Command</span>
            </div>
            <Button variant="ghost" size="icon" className="titlebar-no-drag h-7 w-7" onClick={() => setNewCombOpen(true)}>
              <Plus className="h-4 w-4" />
            </Button>
          </div>
          <Button variant="outline" className="titlebar-no-drag w-full justify-start gap-2" onClick={() => setAddProjectOpen(true)}>
            <FolderGit2 className="h-4 w-4" />
            Adicionar projeto
          </Button>
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto">
          <div className="p-2">
            {combsLoading ? (
              <div className="flex justify-center py-8">
                <Loader2 className="h-5 w-5 animate-spin text-sidebar-foreground/40" />
              </div>
            ) : combs.length === 0 ? (
              <p className="px-3 py-4 text-center text-xs text-sidebar-foreground/40">Nenhum workspace neste projeto.</p>
            ) : (
              <div className="space-y-1">
                {combs.map((comb) => {
                  const attentionForComb = unreadAttentionByCombId.get(comb.id);
                  const isActive = comb.id === activeCombId || comb.id === pointerSelectedCombId;
                  const projectName = projectNameById.get(comb.projectId) ?? "Projeto";
                  const runningCount = projectActivity.runningPanesByCombId[comb.id] ?? 0;
                  return (
                    <WorkspaceListItem
                      key={comb.id}
                      comb={comb}
                      isActive={isActive}
                      projectName={projectName}
                      hasAttention={!!attentionForComb}
                      runningCount={runningCount}
                      attentionExcerpt={attentionForComb?.excerpt ?? null}
                      onSelect={handleSelectWorkspace}
                      onSelectBegin={handleWorkspacePointerDown}
                      onRemove={handleRemoveWorkspaceById}
                      onTogglePin={handleTogglePin}
                    />
                  );
                })}
              </div>
            )}
          </div>
        </div>

        <div className="max-h-[48%] shrink-0 overflow-y-auto border-t border-sidebar-border bg-sidebar/95 px-3 py-3">
          <div className="grid grid-cols-2 gap-2">
            <Button
              variant="ghost"
              size="sm"
              className="titlebar-no-drag w-full justify-start gap-2"
              onClick={() => setAttentionOpen(true)}
            >
              <Bell className="h-4 w-4" />
              Notificações
              {unreadCount > 0 ? <Badge className="ml-auto">{unreadCount}</Badge> : null}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="titlebar-no-drag w-full justify-start gap-2"
              onClick={() => setShowProviders((prev) => !prev)}
            >
              <Settings className="h-4 w-4" />
              Providers
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="titlebar-no-drag w-full justify-start gap-2"
              onClick={() => setCommandPaletteOpen(true)}
            >
              <WandSparkles className="h-4 w-4" />
              Palette
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="titlebar-no-drag w-full justify-start gap-2"
              onClick={() => setRepoConfigOpen(true)}
            >
              <Settings className="h-4 w-4" />
              Repo
            </Button>
          </div>

          <div className="mt-3 rounded-lg border border-sidebar-border bg-sidebar-accent/30 px-3 py-3">
            <div className="mb-2 flex items-start justify-between gap-2">
              <div className="min-w-0">
                <p className="text-xs font-semibold uppercase tracking-wide text-sidebar-foreground/60">
                  Command Center
                </p>
                <p className="truncate text-[11px] text-sidebar-foreground/50">
                  {activeProject ? activeProject.name : "Nenhum projeto ativo"}
                </p>
              </div>
              <Badge variant="outline" className="border-sidebar-border text-[10px]">
                {projectActivity.totalRunningPanes} ativos
              </Badge>
            </div>

            {activeProject ? (
              <div className="space-y-3">
                <div className="rounded-md border border-sidebar-border/70 bg-sidebar-accent/40 px-3 py-2">
                  <p className="text-xs font-medium">{getProjectConfigBranchPrefix(activeRepoConfig)}</p>
                  <p className="mt-0.5 text-[11px] text-sidebar-foreground/60">
                    Prefixo de branch do repo
                  </p>
                  <p className="mt-2 text-[11px] text-sidebar-foreground/70">
                    Agente padrão{" "}
                    <span className="font-medium">
                      {activeRepoConfig?.defaultAgentProviderId
                        ? (providerById.get(activeRepoConfig.defaultAgentProviderId)?.name ?? "definido")
                        : "não definido"}
                    </span>
                  </p>
                </div>

                <div className="rounded-md border border-sidebar-border/70 bg-sidebar-accent/30 px-3 py-2">
                  <div className="flex items-center gap-2 text-xs font-medium">
                    <Clock3 className="h-3.5 w-3.5" />
                    Daemon local
                  </div>
                  <p className="mt-1 text-[11px] text-sidebar-foreground/60">
                    {daemonStatus?.running ? "Executando" : "Parado"}{" "}
                    {daemonStatus?.lastTickAt ? `· update ${daemonStatus.lastTickAt}` : ""}
                  </p>
                  <p className="mt-1 text-[11px] text-sidebar-foreground/70">
                    {daemonStatus?.runningTasks ?? 0} em execução ·{" "}
                    {daemonStatus?.enabledTasks ?? activeRepoTasks.length} tarefas habilitadas
                  </p>
                </div>

                <div className="rounded-md border border-sidebar-border/70 bg-sidebar-accent/20 px-3 py-2">
                  <p className="text-[10px] font-semibold uppercase tracking-wide text-sidebar-foreground/55">
                    Ações do projeto
                  </p>
                  <div className="mt-2 flex flex-wrap gap-2">
                    {activeRepoConfig?.setupCommand?.trim() ? (
                      <Button
                        variant="secondary"
                        size="sm"
                        className="titlebar-no-drag min-w-[120px] flex-1 justify-start gap-2"
                        onClick={() =>
                          void launchManagedCommand({
                            title: "Setup",
                            command: activeRepoConfig.setupCommand ?? "",
                            cwdMode: "project",
                            description: "Executa o setup do repositório",
                          })
                        }
                      >
                        <WandSparkles className="h-3.5 w-3.5" />
                        Setup
                      </Button>
                    ) : null}
                    {activeRepoConfig?.teardownCommand?.trim() ? (
                      <Button
                        variant="outline"
                        size="sm"
                        className="titlebar-no-drag min-w-[120px] flex-1 justify-start gap-2"
                        onClick={() =>
                          void launchManagedCommand({
                            title: "Teardown",
                            command: activeRepoConfig.teardownCommand ?? "",
                            cwdMode: "project",
                            description: "Executa o teardown do repositório",
                          })
                        }
                      >
                        <Terminal className="h-3.5 w-3.5" />
                        Teardown
                      </Button>
                    ) : null}
                  </div>
                  {!(activeRepoConfig?.setupCommand?.trim() || activeRepoConfig?.teardownCommand?.trim()) ? (
                    <p className="mt-2 text-[11px] text-sidebar-foreground/45">
                      Sem ações globais configuradas.
                    </p>
                  ) : null}
                </div>

                <div className="space-y-2">
                  <SidebarSection
                    title="Explorer do daemon"
                    description="Snapshot de combs, panes e diffs"
                    count={`${daemonCombs.length}/${daemonPanes.length}/${daemonDiffBundle.length}`}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <p className="text-[11px] text-sidebar-foreground/55">
                        {daemonExplorerLoading
                          ? "Sincronizando combs, panes e diffs…"
                          : "Visão rápida do estado do daemon"}
                      </p>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="titlebar-no-drag h-6 w-6"
                        onClick={() => void refreshDaemonExplorer()}
                      >
                        <RefreshCw className={`h-3 w-3 ${daemonExplorerLoading ? "animate-spin" : ""}`} />
                      </Button>
                    </div>

                    <div className="mt-2 space-y-2">
                      <div className="space-y-1">
                        <p className="text-[10px] font-semibold uppercase tracking-wide text-sidebar-foreground/55">
                          Combs
                        </p>
                        {daemonCombs.length === 0 ? (
                          <p className="text-[10px] text-sidebar-foreground/45">Sem combs no snapshot.</p>
                        ) : (
                          <div className="space-y-1">
                            {daemonCombs.slice(0, 4).map((comb) => (
                              <button
                                key={comb.id}
                                type="button"
                                className="flex w-full items-center justify-between gap-2 rounded-md border border-transparent px-2 py-1 text-left text-[11px] hover:border-sidebar-border hover:bg-sidebar-accent/40"
                                onClick={() => {
                                  setSelectedProjectId(comb.projectId);
                                  setActiveCombId(comb.id);
                                  setShowProviders(false);
                                }}
                              >
                                <span className="truncate">{comb.name}</span>
                                <span className="shrink-0 text-[10px] text-sidebar-foreground/55">
                                  {comb.worktreePath ? "worktree" : "sem worktree"}
                                </span>
                              </button>
                            ))}
                          </div>
                        )}
                      </div>

                      <div className="space-y-1">
                        <p className="text-[10px] font-semibold uppercase tracking-wide text-sidebar-foreground/55">
                          Panes
                        </p>
                        {daemonPanes.length === 0 ? (
                          <p className="text-[10px] text-sidebar-foreground/45">Sem panes no snapshot.</p>
                        ) : (
                          <div className="space-y-1">
                            {daemonPanes.slice(0, 4).map((pane) => {
                              const comb = combById.get(pane.combId) ?? daemonCombs.find((item) => item.id === pane.combId) ?? null;
                              const label = pane.title ?? (pane.type === "agent" ? "Agent" : "Terminal");
                              return (
                                <button
                                  key={pane.id}
                                  type="button"
                                  className="flex w-full items-center justify-between gap-2 rounded-md border border-transparent px-2 py-1 text-left text-[11px] hover:border-sidebar-border hover:bg-sidebar-accent/40"
                                  onClick={() => {
                                    if (comb) {
                                      setSelectedProjectId(comb.projectId);
                                      setActiveCombId(comb.id);
                                    }
                                    setActivePaneId(pane.id);
                                    setShowProviders(false);
                                  }}
                                >
                                  <span className="truncate">{label}</span>
                                  <span className="shrink-0 text-[10px] text-sidebar-foreground/55">
                                    {pane.status}
                                  </span>
                                </button>
                              );
                            })}
                          </div>
                        )}
                      </div>

                      <div className="space-y-1">
                        <p className="text-[10px] font-semibold uppercase tracking-wide text-sidebar-foreground/55">
                          Diffs
                        </p>
                        {daemonDiffBundle.length === 0 ? (
                          <p className="text-[10px] text-sidebar-foreground/45">Sem bundle de diffs disponível.</p>
                        ) : (
                          <div className="space-y-1">
                            {daemonDiffBundle.slice(0, 4).map((item) => (
                              <div
                                key={item.worktreePath}
                                className="rounded-md border border-sidebar-border/70 bg-sidebar-accent/20 px-2 py-1 text-[10px]"
                              >
                                <div className="flex items-start justify-between gap-2">
                                  <span className="truncate">{item.worktreePath}</span>
                                  <span className={`shrink-0 ${item.success ? "text-emerald-500" : "text-destructive"}`}>
                                    {item.success ? "ok" : "erro"}
                                  </span>
                                </div>
                                <p className="mt-0.5 text-[10px] text-sidebar-foreground/55">
                                  {item.success && item.summary
                                    ? `${item.summary.changedFiles} arquivos · +${item.summary.insertions} -${item.summary.deletions}`
                                    : item.error ?? "Sem resumo"}
                                </p>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    </div>
                  </SidebarSection>

                  <SidebarSection
                    title="Processos"
                    description="Supervisor (daemon), métricas e atalhos no terminal"
                    defaultOpen={(activeRepoConfig?.processes?.length ?? 0) > 0}
                    count={`${activeRepoConfig?.processes?.length ?? 0}`}
                  >
                    {activeProject && daemonApi?.listProcesses ? (
                      <div className="-mx-1 max-h-[min(42vh,400px)] overflow-x-hidden overflow-y-auto pr-0.5">
                        <ProcessesPanel project={activeProject} embedded refreshInterval={4000} />
                      </div>
                    ) : null}

                    {(activeRepoConfig?.processes ?? []).length === 0 ? (
                      <p className="text-[11px] text-sidebar-foreground/45">
                        Sem processos configurados.
                      </p>
                    ) : (
                      <>
                        {activeProject && daemonApi?.listProcesses ? (
                          <p className="mb-1.5 mt-2 text-[10px] font-semibold uppercase tracking-wide text-sidebar-foreground/55">
                            Abrir no terminal
                          </p>
                        ) : null}
                        <div className="space-y-1">
                          {activeRepoConfig?.processes?.map((process) => (
                            <Button
                              key={process.id}
                              variant="ghost"
                              size="sm"
                              className="titlebar-no-drag w-full justify-start gap-2 border border-transparent px-2 text-left hover:border-sidebar-border"
                              onClick={() =>
                                void launchManagedCommand({
                                  title: process.name,
                                  command: process.command,
                                  cwdMode: process.cwdMode ?? "worktree",
                                  description: process.description ?? null,
                                })
                              }
                            >
                              <Workflow className="h-3.5 w-3.5" />
                              <span className="truncate">{process.name}</span>
                            </Button>
                          ))}
                        </div>
                      </>
                    )}
                  </SidebarSection>

                  <SidebarSection
                    title="Presets"
                    description="Executa comandos prontos"
                    count={`${activeRepoConfig?.presets?.length ?? 0}`}
                  >
                    {(activeRepoConfig?.presets ?? []).length === 0 ? (
                      <p className="text-[11px] text-sidebar-foreground/45">
                        Sem presets configurados.
                      </p>
                    ) : (
                      <div className="space-y-1">
                        {activeRepoConfig?.presets?.map((preset) => (
                          <Button
                            key={preset.id}
                            variant="ghost"
                            size="sm"
                            className="titlebar-no-drag w-full justify-start gap-2 border border-transparent px-2 text-left hover:border-sidebar-border"
                            onClick={() =>
                              void launchManagedCommand({
                                title: preset.name,
                                command: preset.command,
                                cwdMode: "worktree",
                                description: preset.description ?? null,
                              })
                            }
                          >
                            <WandSparkles className="h-3.5 w-3.5" />
                            <span className="truncate">{preset.name}</span>
                          </Button>
                        ))}
                      </div>
                    )}
                  </SidebarSection>

                  <SidebarSection
                    title="Tarefas"
                    description="Agendamentos do projeto"
                    defaultOpen={activeRepoTasks.length > 0}
                    count={`${activeRepoTasks.length}`}
                  >
                    {activeRepoTasks.length === 0 ? (
                      <p className="text-[11px] text-sidebar-foreground/45">
                        Sem tarefas agendadas.
                      </p>
                    ) : (
                      <div className="space-y-1">
                        {activeRepoTasks.map((task) => {
                          const runtime = daemonTaskById.get(task.id);
                          return (
                            <div
                              key={task.id}
                              className="rounded-lg border border-sidebar-border/70 bg-sidebar-accent/25 px-2 py-2"
                            >
                              <div className="flex items-start justify-between gap-2">
                                <div className="min-w-0">
                                  <p className="truncate text-[12px] font-medium">{task.name}</p>
                                  <p className="truncate text-[10px] text-sidebar-foreground/55">
                                    {task.schedule}
                                  </p>
                                </div>
                                <Badge
                                  variant={task.enabled === false ? "outline" : "secondary"}
                                  className="shrink-0 text-[10px]"
                                >
                                  {task.enabled === false ? "desativada" : runtime?.status ?? "idle"}
                                </Badge>
                              </div>
                              <div className="mt-2 flex items-center gap-2">
                                <Button
                                  type="button"
                                  size="sm"
                                  variant="outline"
                                  className="titlebar-no-drag h-7 px-2 text-[11px]"
                                  onClick={() => void runRepoTask(task)}
                                >
                                  <Clock3 className="mr-1 h-3 w-3" />
                                  Executar
                                </Button>
                                {runtime ? (
                                  <Button
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className="titlebar-no-drag h-7 px-2 text-[11px]"
                                    onClick={async () => {
                                      if (!daemonApi) return;
                                      try {
                                        const next = runtime.attached
                                          ? await daemonApi.detachTask(activeProject?.id ?? "", task.id)
                                          : await daemonApi.attachTask(activeProject?.id ?? "", task.id);
                                        if (!next.success) {
                                          toast.error(next.error ?? "Falha ao alternar attachment");
                                          return;
                                        }
                                        setDaemonTasks((prev) =>
                                          prev.map((item) =>
                                            item.projectId === runtime.projectId && item.taskId === task.id
                                              ? (next.task ?? { ...item, attached: !item.attached })
                                              : item,
                                          ),
                                        );
                                      } catch (error) {
                                        toast.error(error instanceof Error ? error.message : "Falha ao alternar attachment");
                                      }
                                    }}
                                  >
                                    {runtime.attached ? "Desanexar" : "Anexar"}
                                  </Button>
                                ) : null}
                                {runtime?.nextRunAt ? (
                                  <span className="text-[10px] text-sidebar-foreground/55">
                                    próxima {runtime.nextRunAt}
                                  </span>
                                ) : null}
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    )}
                  </SidebarSection>
                </div>
              </div>
            ) : (
              <p className="text-[11px] text-sidebar-foreground/45">
                Adiciona um projeto para expor processos e presets no workspace.
              </p>
            )}
          </div>
        </div>
      </aside>

      <main className="flex min-h-0 flex-1 flex-col overflow-hidden" data-workspace-main>
        {showProviders ? (
          <div className="flex-1 overflow-auto mt-8">
            <SettingsPage />
          </div>
        ) : activeComb ? (
          <>
            <div className="mt-8 flex items-center justify-between border-b border-border px-4 py-2">
              <div className="flex min-w-0 items-center gap-3">
                <h3 className="truncate text-sm font-semibold">{activeComb.name}</h3>
                <Badge variant="outline" className="gap-1">
                  <GitBranch className="h-3 w-3" />
                  {activeComb.branch ?? activeComb.baseBranch}
                </Badge>
              </div>
              <div className="flex items-center gap-2">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="sm" onClick={handleOpenBaseTerminal}>
                      <FolderGit2 className="mr-1 h-3.5 w-3.5" />
                      Base
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Terminal no repositorio principal</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="sm" onClick={handleAddTerminal}>
                      <Terminal className="mr-1 h-3.5 w-3.5" />
                      Workspace
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Terminal no worktree atual</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="sm" onClick={() => setNewAgentOpen(true)} disabled={cliProviders.length === 0}>
                      <Bot className="mr-1 h-3.5 w-3.5" />
                      Agent
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Abrir agente CLI no workspace</TooltipContent>
                </Tooltip>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-hidden">
              {panesLoading ? (
                <div className="flex h-full flex-col items-center justify-center gap-3 p-8">
                  <Loader2 className="h-10 w-10 animate-spin text-muted-foreground/35" />
                  <p className="text-sm text-muted-foreground">A abrir workspace…</p>
                </div>
              ) : visiblePanes.length === 0 ? (
                <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
                  <Terminal className="h-12 w-12 text-muted-foreground/30" />
                  <p className="text-sm text-muted-foreground">Nenhum pane aberto neste workspace</p>
                </div>
              ) : (
                <div className="flex h-full min-h-0 flex-col overflow-hidden p-1">
                  <div role="tablist" aria-label="Panes do workspace" className="mb-1 flex shrink-0 gap-1 overflow-x-auto">
                    {visiblePanes.map((pane) => {
                      const provider = pane.providerId ? (providerById.get(pane.providerId) ?? null) : null;
                      const label = pane.type === "agent" ? (pane.title ?? provider?.name ?? "Agent") : (pane.title ?? "Terminal");
                      const selected = pane.id === activePane?.id;
                      const hasUnreadAttention = hasUnreadAttentionByPaneId.has(pane.id);
                      return (
                        <div
                          key={pane.id}
                          role="tab"
                          aria-selected={selected}
                          onClick={() => handleSelectPaneTab(pane.id)}
                          className={`group flex min-w-[170px] max-w-[260px] cursor-pointer items-center gap-2 rounded-md border px-2 py-1.5 ${
                            selected ? "border-primary bg-primary/10" : "border-border bg-muted/30 hover:bg-muted/50"
                          }`}
                        >
                          {pane.type === "agent" ? <Bot className="h-3.5 w-3.5 shrink-0" /> : <Terminal className="h-3.5 w-3.5 shrink-0" />}
                          {pane.type === "agent" ? <AgentKindBadge provider={provider} compact /> : null}
                          <span className="min-w-0 flex-1 truncate text-xs">{label}</span>
                          {hasUnreadAttention ? <span className="h-2 w-2 shrink-0 rounded-full bg-sky-400" /> : null}
                          <Button
                            variant="ghost"
                            size="icon"
                            className="ml-auto h-5 w-5 shrink-0 opacity-70 hover:opacity-100"
                            onClick={(event) => {
                              event.stopPropagation();
                              handleRemovePaneById(pane.id);
                            }}
                          >
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </div>
                      );
                    })}
                  </div>
                  <div className="min-h-0 flex-1 overflow-hidden">
                    {activePane ? (
                      <PaneCard
                        pane={activePane}
                        worktreePath={activePane.cwd?.trim() || activeComb.worktreePath || ""}
                        provider={activePane.providerId ? (providerById.get(activePane.providerId) ?? null) : null}
                        onPaneStatusChange={handlePaneStatusChange}
                        onRemovePane={handleRemovePaneById}
                      />
                    ) : null}
                  </div>
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="mt-8 flex h-full flex-col items-center justify-center gap-4 p-8">
            <FolderGit2 className="h-16 w-16 text-muted-foreground/20" />
            <div className="text-center">
              <h3 className="text-lg font-medium">Crie ou selecione um workspace</h3>
              <p className="mt-1 text-sm text-muted-foreground">Fluxo simplificado: workspaces, panes e providers.</p>
              <div className="mt-2">
                <WorkflowTip centered />
              </div>
            </div>
            <Button onClick={() => setNewCombOpen(true)}>
              <Plus className="mr-2 h-4 w-4" />
              Novo Workspace
            </Button>
          </div>
        )}
      </main>

      <Dialog open={attentionOpen} onOpenChange={setAttentionOpen}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>Notificações</DialogTitle>
            <DialogDescription>Eventos recentes de atenção dos agentes e terminais.</DialogDescription>
          </DialogHeader>
          <div className="max-h-[60vh] space-y-2 overflow-auto">
            {attentionRecords.length === 0 ? (
              <p className="text-sm text-muted-foreground">Sem notificações.</p>
            ) : (
              attentionRecords.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className="w-full rounded border border-border p-3 text-left hover:bg-muted/50"
                  onClick={() => {
                    setSelectedProjectId(item.projectId);
                    setActiveCombId(item.combId);
                    setActivePaneId(item.paneId);
                    setShowProviders(false);
                    setAttentionRecords((prev) => prev.map((r) => (r.id === item.id ? { ...r, read: true } : r)));
                    setAttentionOpen(false);
                  }}
                >
                  <div className="flex items-center justify-between gap-3">
                    <p className="text-sm font-medium">
                      {item.projectName} · {item.workspaceName}
                    </p>
                    {!item.read ? <Badge>Novo</Badge> : null}
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">{item.excerpt ?? "Agente aguardando interação."}</p>
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    {formatDistanceToNow(new Date(item.createdAt), { addSuffix: true, locale: ptBR })}
                  </p>
                </button>
              ))
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setAttentionRecords((prev) => prev.map((item) => ({ ...item, read: true })))}
            >
              Marcar tudo como lido
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <NewWorkspaceDialog
        open={newCombOpen}
        onOpenChange={setNewCombOpen}
        projects={sortedProjects}
        selectedProjectId={selectedProjectId}
        onCreate={(comb) => {
          setSelectedProjectId(comb.projectId);
          setActiveCombId(comb.id);
          setShowProviders(false);
          refreshCombs();
        }}
        createComb={createComb}
      />

      {activeCombId ? (
        <NewAgentPaneDialog
          open={newAgentOpen}
          onOpenChange={setNewAgentOpen}
          combId={activeCombId}
          providers={providers}
          preferredProviderId={activeRepoConfig?.defaultAgentProviderId ?? activeProject?.defaultProviderId ?? null}
          ensureCombWorktree={ensureActiveCombWorktree}
          onCreate={async (pane) => {
            await refreshPanes();
            setActivePaneId(pane.id);
          }}
        />
      ) : null}

      <AddProjectDialog open={addProjectOpen} onOpenChange={setAddProjectOpen} />

      <ProjectRepoConfigDialog
        open={repoConfigOpen}
        onOpenChange={setRepoConfigOpen}
        project={activeProject}
        providers={providers}
        onEditToml={() => setRepoConfigTomlOpen(true)}
        onSave={async (projectId, config) => {
          await updateProject(projectId, { repoConfig: config });
          await refreshProjects();
          await refreshCombs();
        }}
      />

      <ProjectRepoTomlDialog
        open={repoConfigTomlOpen}
        onOpenChange={setRepoConfigTomlOpen}
        project={activeProject}
        onSaved={async () => {
          await refreshProjects();
          await refreshCombs();
        }}
      />

      <WorkspaceCommandPalette
        open={commandPaletteOpen}
        onOpenChange={setCommandPaletteOpen}
        projects={sortedProjects}
        combs={combs}
        panes={visiblePanes}
        activeProject={activeProject}
        activeCombId={activeCombId}
        activePaneId={activePaneId}
        repoConfig={activeRepoConfig}
        taskTemplates={taskTemplates}
        tasks={activeRepoTasks}
        onOpenSettings={() => setShowProviders(true)}
        onOpenNewWorkspace={() => setNewCombOpen(true)}
        onOpenBaseTerminal={handleOpenBaseTerminal}
        onOpenWorkspaceTerminal={handleAddTerminal}
        onOpenNewAgent={() => setNewAgentOpen(true)}
        onOpenRepoConfig={() => setRepoConfigOpen(true)}
        onSelectProject={(projectId) => {
          setSelectedProjectId(projectId);
          const nextProjectComb = combs.find((comb) => comb.projectId === projectId) ?? null;
          setActiveCombId(nextProjectComb?.id ?? null);
        }}
        onSelectWorkspace={(combId) => {
          const comb = combs.find((item) => item.id === combId);
          if (!comb) return;
          setSelectedProjectId(comb.projectId);
          setActiveCombId(comb.id);
        }}
        onSelectPane={(paneId) => {
          setActivePaneId(paneId);
          markPaneAttentionAsRead(paneId);
        }}
        onLaunchCommand={launchManagedCommand}
      />
    </div>
  );
}
