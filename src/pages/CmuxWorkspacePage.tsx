"use client";

import React, { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { DragDropContext, Droppable, Draggable, type DropResult } from "@hello-pangea/dnd";
import {
  Bell,
  Bot,
  Clock3,
  ChevronLeft,
  ChevronRight,
  ChevronDown,
  Cpu,
  Database,
  FolderGit2,
  GitBranch,
  GitPullRequest,
  Loader2,
  Merge,
  MemoryStick,
  Pin,
  Plus,
  RefreshCw,
  Settings,
  Tag,
  Terminal,
  Trash2,
  WandSparkles,
  Workflow,
} from "lucide-react";
import { toast } from "sonner";
import { format, formatDistanceToNow, isValid } from "date-fns";
import { ptBR } from "date-fns/locale";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
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
import { useTheme } from "@/components/theme-provider";
import { CombReviewPanel } from "@/components/review/comb-review-panel";
import { GitActionsPanel } from "@/components/review/git-actions-panel";
import { CommitDialog } from "@/components/dialogs/commit-dialog";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";
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
  UpdateCombDTO,
  UpdatePaneDTO,
} from "@/lib/database/types";
import type {
  DaemonDiffBundleItem,
  DaemonStatus,
  DaemonTaskStatus,
  DetectedTerminalAgent,
  GitStatus,
} from "@/types/app";
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
import { useWorktreeNavigationHistory } from "@/hooks/use-worktree-navigation-history";
import { PaneTab } from "@/components/pane-tab";
import { formatRelativeTimeFromNow } from "@/lib/format-relative-time";
import { DiffViewer } from "@/components/review/diff-viewer";

const CLI_PROVIDER_TYPES = ["codex", "claude-code", "gemini", "cursor"] as const;

const activePaneStorageKey = (combId: string) => `dcc:workspace:${combId}:activePane`;
const activeWorkspaceViewStorageKey = (combId: string) => `dcc:workspace:${combId}:mainView`;

type WorkspaceMainView = "panes" | "review";
type ReviewSummary = {
  changedFiles: number;
  insertions: number;
  deletions: number;
};

function isCliProviderType(type: string): type is (typeof CLI_PROVIDER_TYPES)[number] {
  return CLI_PROVIDER_TYPES.includes(type as (typeof CLI_PROVIDER_TYPES)[number]);
}

function formatDaemonMemory(mb: number): string {
  if (mb < 1) return `${(mb * 1024).toFixed(0)} KB`;
  if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
  return `${mb.toFixed(1)} MB`;
}

function formatDaemonLastTick(iso: string): string {
  const d = new Date(iso);
  return isValid(d) ? format(d, "dd/MM/yyyy 'às' HH:mm:ss", { locale: ptBR }) : iso;
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

type WorkspaceRemovalDialogState = {
  combId: string;
  title: string;
  description: string;
  confirmLabel: string;
  confirmVariant: "default" | "destructive";
  isRemoving: boolean;
};

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
  const [namingPreview, setNamingPreview] = useState<{
    branchPrefix: string;
    slug: string;
    idSuffixExample: string;
    branch: string;
    worktreePath: string;
  } | null>(null);
  const [namingPreviewLoading, setNamingPreviewLoading] = useState(false);
  const [forgeIssueRef, setForgeIssueRef] = useState("");
  const [forgeToken, setForgeToken] = useState("");
  const [forgeLoading, setForgeLoading] = useState(false);

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
    setForgeIssueRef("");
    setForgeToken("");
    setForgeLoading(false);
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

  useEffect(() => {
    if (!open) {
      setNamingPreview(null);
      setNamingPreviewLoading(false);
      return;
    }
    const api = window.desktopAPI?.comb?.previewWorktreeNaming;
    if (!api || !projectId) {
      setNamingPreview(null);
      setNamingPreviewLoading(false);
      return;
    }
    let cancelled = false;
    setNamingPreviewLoading(true);
    const id = window.setTimeout(() => {
      void api(projectId, name)
        .then((data) => {
          if (!cancelled) setNamingPreview(data);
        })
        .catch(() => {
          if (!cancelled) setNamingPreview(null);
        })
        .finally(() => {
          if (!cancelled) setNamingPreviewLoading(false);
        });
    }, 200);
    return () => {
      cancelled = true;
      window.clearTimeout(id);
    };
  }, [open, projectId, name]);

  const handleForgeFetch = async () => {
    const ref = forgeIssueRef.trim();
    if (!ref || !projectId) {
      toast.error("Indica o URL ou owner/repo#123 da issue.");
      return;
    }
    const api = window.desktopAPI?.forge?.fetchIssue;
    if (!api) {
      toast.error("Carregar issues só está disponível na aplicação desktop.");
      return;
    }
    setForgeLoading(true);
    try {
      const data = await api(projectId, ref, forgeToken.trim() || undefined);
      const desc = (data.suggestedDescription ?? "").slice(0, 12000);
      setName(data.suggestedWorkspaceName);
      setDescription(desc);
      toast.success("Issue carregada — revê o nome e cria o workspace.");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "Falha ao carregar a issue");
    } finally {
      setForgeLoading(false);
    }
  };

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
          {projectId && window.desktopAPI?.forge?.fetchIssue ? (
            <div className="space-y-2 rounded-md border border-border bg-muted/15 px-3 py-2">
              <div className="flex items-center gap-2 text-xs font-medium text-foreground">
                <Tag className="h-3.5 w-3.5 shrink-0 opacity-70" />
                Issue (GitHub / GitLab)
              </div>
              <p className="text-[10px] leading-snug text-muted-foreground">
                Cola o URL da issue, ou <span className="font-mono">owner/repo#123</span> (GitHub). Repositórios
                privados: token nas variáveis de ambiente ou abaixo.
              </p>
              <div className="flex flex-col gap-2 sm:flex-row">
                <Input
                  value={forgeIssueRef}
                  onChange={(e) => setForgeIssueRef(e.target.value)}
                  placeholder="https://github.com/org/repo/issues/42 ou org/repo#42"
                  disabled={forgeLoading}
                  className="sm:flex-1"
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleForgeFetch();
                  }}
                />
                <Button
                  type="button"
                  variant="secondary"
                  className="shrink-0"
                  disabled={forgeLoading || !forgeIssueRef.trim()}
                  onClick={() => void handleForgeFetch()}
                >
                  {forgeLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : "Carregar"}
                </Button>
              </div>
              <Input
                type="password"
                value={forgeToken}
                onChange={(e) => setForgeToken(e.target.value)}
                placeholder="Token opcional (PAT) — não é guardado"
                disabled={forgeLoading}
                autoComplete="off"
                className="font-mono text-xs"
              />
            </div>
          ) : null}
          <div className="space-y-2">
            <label className="text-sm font-medium">Nome</label>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="ex.: auth-refactor" />
          </div>
          {projectId && window.desktopAPI?.comb?.previewWorktreeNaming ? (
            <div className="space-y-1.5 rounded-md border border-border bg-muted/20 px-3 py-2">
              <p className="text-xs font-medium text-foreground">Branch e pasta (pré-visualização)</p>
              {namingPreviewLoading ? (
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin" />
                  A calcular…
                </div>
              ) : namingPreview ? (
                <>
                  <p
                    className="break-all font-mono text-[11px] leading-snug text-muted-foreground"
                    title={namingPreview.branch}
                  >
                    <span className="text-foreground/85">Branch:</span> {namingPreview.branch}
                  </p>
                  <p
                    className="break-all font-mono text-[11px] leading-snug text-muted-foreground"
                    title={namingPreview.worktreePath}
                  >
                    <span className="text-foreground/85">Pasta:</span> {namingPreview.worktreePath}
                  </p>
                  <p className="text-[10px] leading-snug text-muted-foreground/90">
                    O nome é sanitizado para Git (não-alfanuméricos → hífen). O sufixo final de 8 caracteres
                    hexadecimais vem do ID do workspace ao criar; aqui mostra-se um valor de exemplo (
                    {namingPreview.idSuffixExample}).
                  </p>
                </>
              ) : (
                <p className="text-xs text-muted-foreground">Não foi possível pré-visualizar.</p>
              )}
            </div>
          ) : null}
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
  prepareWorkspace,
  updatePane,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  combId: string;
  providers: Provider[];
  preferredProviderId?: string | null;
  onCreate: (pane: Pane) => void;
  ensureCombWorktree: (combId: string) => Promise<string | null>;
  prepareWorkspace: (combId: string, task: () => Promise<void>) => Promise<void>;
  updatePane: (paneId: string, data: UpdatePaneDTO) => Promise<void>;
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
      await prepareWorkspace(combId, async () => {
        const worktreePath = await ensureCombWorktree(combId);
        if (!worktreePath) return;
        const pane = await create({
          combId,
          type: "agent",
          providerId: providerId || undefined,
        });
        await updatePane(pane.id, { cwd: worktreePath });
        onCreate(pane);
        onOpenChange(false);
        toast.success("Agent pane criado");
      });
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
  reviewSummary,
  runningCount,
  agentCount,
  agentPreview,
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
  reviewSummary?: ReviewSummary | null;
  runningCount: number;
  agentCount: number;
  agentPreview: DetectedTerminalAgent[];
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
          {comb.forgeLink?.url ? (
            <div className="mt-0.5 flex min-w-0 items-start gap-1">
              <GitPullRequest className="mt-0.5 h-3 w-3 shrink-0 text-sidebar-foreground/55" />
              <button
                type="button"
                className="line-clamp-2 text-left text-[10px] text-sky-600 underline-offset-2 hover:underline dark:text-sky-400"
                title={comb.forgeLink.title ?? comb.forgeLink.url}
                onClick={(e) => {
                  e.stopPropagation();
                  void window.desktopAPI?.shell?.openExternal(comb.forgeLink!.url);
                }}
              >
                {comb.forgeLink.forge === "gitlab" ? "MR" : "PR"} #{comb.forgeLink.number}
                {comb.forgeLink.title
                  ? ` · ${comb.forgeLink.title.length > 48 ? `${comb.forgeLink.title.slice(0, 48)}…` : comb.forgeLink.title}`
                  : ""}
              </button>
            </div>
          ) : null}
          {comb.lastGitActivityAt ? (
            <p className="mt-0.5 text-[10px] text-sidebar-foreground/45" title="Última atividade Git no worktree">
              {formatRelativeTimeFromNow(comb.lastGitActivityAt) ?? ""}
            </p>
          ) : null}
          <p className="mt-0.5 line-clamp-1 text-[10px] text-sidebar-foreground/50">{projectName}</p>
          {runningCount > 0 ? (
            <Badge variant="outline" className="mt-1 h-5 border-sidebar-border px-1.5 text-[10px] text-sidebar-foreground/70">
              {runningCount} ativos
            </Badge>
          ) : null}
          {reviewSummary && reviewSummary.changedFiles > 0 ? (
            <Badge
              variant="secondary"
              className="mt-1 h-5 px-1.5 text-[10px]"
              title="Quantidade de ficheiros alterados no review deste workspace"
            >
              Review · {reviewSummary.changedFiles} +{reviewSummary.insertions}
              /-{reviewSummary.deletions}
            </Badge>
          ) : null}
          {agentCount > 0 ? (
            <div className="mt-1 flex flex-wrap gap-1">
              {agentPreview.map((agent) => (
                <Badge
                  key={`${agent.ptyId}:${agent.agentKind}`}
                  variant={agent.status === "waiting" ? "destructive" : "secondary"}
                  className="h-5 px-1.5 text-[10px]"
                  title={`${agent.agentLabel} · ${agent.cwd}`}
                >
                  {agent.agentLabel}
                  <span className="ml-1 opacity-70">{agent.status}</span>
                </Badge>
              ))}
              {agentCount > agentPreview.length ? (
                <Badge variant="outline" className="h-5 border-sidebar-border px-1.5 text-[10px] text-sidebar-foreground/70">
                  +{agentCount - agentPreview.length}
                </Badge>
              ) : null}
            </div>
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
  combId,
  projectId,
  onPaneStatusChange,
  onRemovePane,
}: {
  pane: Pane;
  worktreePath: string;
  provider: Provider | null;
  combId?: string;
  projectId?: string;
  onPaneStatusChange: (paneId: string, status: "running" | "exited") => void;
  onRemovePane: (paneId: string) => void;
}) {
  const command = getPaneRuntimeCommand(pane, provider);
  const label = pane.type === "agent" ? (pane.title ?? provider?.name ?? "Agent") : (pane.title ?? "Terminal");
  const args = pane.type === "agent" && pane.initialPrompt ? [pane.initialPrompt] : [];
  const handleRemove = useCallback(() => onRemovePane(pane.id), [onRemovePane, pane.id]);
  /** Sincroniza badge com sessão PTY no backend (reattach ao mudar de pane). */
  const [agentStatus, setAgentStatus] = useState<"running" | "exited" | null>(null);
  const normalizedWorktreePath = worktreePath.trim();

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
        {normalizedWorktreePath ? (
          <EmbeddedTerminal
            cwd={normalizedWorktreePath}
            command={command}
            args={args}
            paneId={pane.id}
            combId={combId}
            projectId={projectId}
            title={label}
            onSessionActive={isAgent ? handleAgentSessionActive : undefined}
            onExit={handleAgentExit}
          />
        ) : (
          <div className="flex h-full min-h-0 items-center justify-center rounded-lg border border-dashed border-border bg-muted/20 px-4 py-8 text-center text-sm text-muted-foreground">
            <div className="flex flex-col items-center gap-2">
              <Loader2 className="h-5 w-5 animate-spin" />
              <p>A preparar o workspace...</p>
            </div>
          </div>
        )}
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
  const { theme, setTheme } = useTheme();

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
  const [workspaceView, setWorkspaceView] = useState<WorkspaceMainView>("panes");
  const [attentionRecords, setAttentionRecords] = useState<TerminalAttentionRecord[]>([]);

  // ── Git sidebar state ────────────────────────────────────────────
  const [gitSidebarFiles, setGitSidebarFiles] = useState<Array<{
    path: string; status: string; diff: string; insertions?: number; deletions?: number;
  }>>([]);
  const [gitSidebarLoading, setGitSidebarLoading] = useState(false);
  const [activeDiffPath, setActiveDiffPath] = useState<string | null>(null);
  const [diffTabActive, setDiffTabActive] = useState(false);
  const [gitSidebarIsPushing, setGitSidebarIsPushing] = useState(false);
  const [gitSidebarIsPulling, setGitSidebarIsPulling] = useState(false);
  const [gitSidebarIsMerging, setGitSidebarIsMerging] = useState(false);
  const [gitSidebarCommitOpen, setGitSidebarCommitOpen] = useState(false);
  const [gitSidebarCommitStatus, setGitSidebarCommitStatus] =
    useState<GitStatus | null>(null);
  const [gitSidebarCommitStatusLoading, setGitSidebarCommitStatusLoading] =
    useState(false);
  const [gitSidebarMergeOpen, setGitSidebarMergeOpen] = useState(false);
  const [gitSidebarWorktreeDirty, setGitSidebarWorktreeDirty] = useState(false);
  const [gitSidebarMainDirty, setGitSidebarMainDirty] = useState(false);
  const [gitSidebarTargetBranch, setGitSidebarTargetBranch] = useState("");
  const [gitSidebarBranchList, setGitSidebarBranchList] = useState<string[]>([]);
  const [initializingBasePaneIds, setInitializingBasePaneIds] = useState<Set<string>>(new Set());
  const [workspacePrepCombId, setWorkspacePrepCombId] = useState<string | null>(null);
  const [workspaceRemovalDialog, setWorkspaceRemovalDialog] = useState<WorkspaceRemovalDialogState | null>(null);
  const [showShortcutHints, setShowShortcutHints] = useState(false);
  /** Feedback imediato na sidebar antes do commit pesado (xterm / área principal). */
  const [pointerSelectedCombId, setPointerSelectedCombId] = useState<string | null>(null);
  const pointerPressClearTimeoutRef = useRef<number | null>(null);
  const hydratedAttentionRef = useRef(false);
  const isMacPlatform =
    typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/i.test(navigator.platform);

  const sortedProjects = useMemo(
    () => [...projects].sort((a, b) => +new Date(b.updatedAt) - +new Date(a.updatedAt)),
    [projects],
  );
  const [combs, setCombs] = useState<Comb[]>([]);
  const [combsLoading, setCombsLoading] = useState(true);
  const refreshCombs = useCallback(async (opts?: { silent?: boolean }) => {
    const silent = opts?.silent ?? false;
    if (!window.db?.combs || sortedProjects.length === 0) {
      setCombs([]);
      if (!silent) setCombsLoading(false);
      return;
    }
    if (!silent) setCombsLoading(true);
    try {
      const chunks = await Promise.all(
        sortedProjects.map((project) => window.db!.combs.findByProject(project.id)),
      );
      const flat = normalizeCombs(chunks.flat()) as Comb[];
      // Ordenar: fixados primeiro (por pinnedAt desc), depois atividade Git recente, depois updatedAt
      flat.sort((a, b) => {
        if (a.isPinned && !b.isPinned) return -1;
        if (!a.isPinned && b.isPinned) return 1;
        if (a.isPinned && b.isPinned) {
          return +new Date(b.pinnedAt ?? 0) - +new Date(a.pinnedAt ?? 0);
        }
        const ta = a.lastGitActivityAt ? +new Date(a.lastGitActivityAt) : NaN;
        const tb = b.lastGitActivityAt ? +new Date(b.lastGitActivityAt) : NaN;
        const ha = Number.isFinite(ta);
        const hb = Number.isFinite(tb);
        if (ha && hb && tb !== ta) return tb - ta;
        if (ha !== hb) return ha ? -1 : 1;
        return +new Date(b.updatedAt) - +new Date(a.updatedAt);
      });
      setCombs(flat);
    } finally {
      if (!silent) setCombsLoading(false);
    }
  }, [sortedProjects]);

  const sortedProjectsRef = useRef(sortedProjects);
  sortedProjectsRef.current = sortedProjects;

  useEffect(() => {
    const api = window.desktopAPI?.comb?.refreshGitActivity;
    if (!api) return;

    const tick = async () => {
      const projects = sortedProjectsRef.current;
      if (projects.length === 0) return;
      await Promise.all(projects.map((p) => api(p.id).catch(() => undefined)));
      await refreshCombs({ silent: true });
    };

    const firstDelay = window.setTimeout(() => {
      void tick();
    }, 2500);
    const id = window.setInterval(() => {
      void tick();
    }, 60_000);
    const onVis = () => {
      if (document.visibilityState === "visible") void tick();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      window.clearTimeout(firstDelay);
      window.clearInterval(id);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [refreshCombs]);

  const createComb = useCallback(async (data: CreateCombDTO) => {
    if (!window.db?.combs) throw new Error("Combs indisponivel");
    const created = await window.db.combs.create(data);
    const comb = normalizeComb(created as unknown as Record<string, unknown>) as unknown as Comb;
    await refreshCombs();
    return comb;
  }, [refreshCombs]);

  const updateComb = useCallback(
    async (combId: string, data: UpdateCombDTO) => {
      if (!window.db?.combs) throw new Error("Combs indisponivel");
      await window.db.combs.update(combId, data);
      await refreshCombs();
    },
    [refreshCombs],
  );

  const combsRef = useRef(combs);
  combsRef.current = combs;
  const activeComb = useMemo(
    () => (activeCombId ? (combs.find((c) => c.id === activeCombId) ?? null) : null),
    [activeCombId, combs],
  );
  useEffect(() => {
    if (!activeCombId || typeof window === "undefined") {
      setWorkspaceView("panes");
      return;
    }
    const stored = window.localStorage.getItem(activeWorkspaceViewStorageKey(activeCombId));
    if (stored === "review" || stored === "panes") {
      setWorkspaceView(stored);
      return;
    }
    setWorkspaceView("panes");
  }, [activeCombId]);
  useEffect(() => {
    if (!activeCombId || typeof window === "undefined") return;
    window.localStorage.setItem(activeWorkspaceViewStorageKey(activeCombId), workspaceView);
  }, [activeCombId, workspaceView]);
  const activeCombIdRef = useRef(activeCombId);
  const lastAutoSelectedProjectIdRef = useRef<string | null>(null);
  const worktreePrepInFlightRef = useRef<string | null>(null);
  const activeCombWorktreeKey = activeComb?.worktreePath?.trim()
    ? `${activeComb.branch ?? ""}@${activeComb.worktreePath}`
    : "";
  useEffect(() => {
    activeCombIdRef.current = activeCombId;
  }, [activeCombId]);
  useEffect(() => {
    if (!activeCombId || !activeCombWorktreeKey) return;
    const api = window.desktopAPI?.forge?.syncPrLink;
    if (!api) return;
    const timer = window.setTimeout(() => {
      void api(activeCombId)
        .then(() => refreshCombs({ silent: true }))
        .catch(() => undefined);
    }, 1400);
    return () => window.clearTimeout(timer);
  }, [activeCombId, activeCombWorktreeKey, refreshCombs]);
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
  const activeAgentsByCombId = useMemo(() => {
    const map = new Map<string, DetectedTerminalAgent[]>();
    for (const agent of projectActivity.activeAgents ?? []) {
      if (!agent.combId) continue;
      const next = map.get(agent.combId) ?? [];
      next.push(agent);
      map.set(agent.combId, next);
    }
    for (const agents of map.values()) {
      agents.sort((a, b) => {
        const statusRank = a.status === b.status ? 0 : a.status === "waiting" ? -1 : 1;
        return statusRank || a.agentLabel.localeCompare(b.agentLabel) || a.cwd.localeCompare(b.cwd);
      });
    }
    return map;
  }, [projectActivity.activeAgents]);
  const {
    panes,
    isLoading: panesLoading,
    refresh: refreshPanes,
    create: createPane,
    update: updatePane,
    remove: removePane,
  } = usePanes(activeCombId ?? undefined);
  useEffect(() => {
    void refreshCombs({ silent: false });
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
  const {
    navigateToComb,
    goBack: goWorktreeBack,
    goForward: goWorktreeForward,
    canGoBack: canGoWorktreeBack,
    canGoForward: canGoWorktreeForward,
  } = useWorktreeNavigationHistory(combs, activeCombId, setActiveCombId, setSelectedProjectId);
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
  const reviewSummaryByWorktreePath = useMemo(() => {
    const map = new Map<string, ReviewSummary>();
    for (const item of daemonDiffBundle) {
      const worktreePath = item.worktreePath?.trim();
      if (!worktreePath) continue;
      if (!item.success || !item.summary) {
        map.set(worktreePath, { changedFiles: 0, insertions: 0, deletions: 0 });
        continue;
      }
      map.set(worktreePath, {
        changedFiles: item.summary.changedFiles ?? 0,
        insertions: item.summary.insertions ?? 0,
        deletions: item.summary.deletions ?? 0,
      });
    }
    return map;
  }, [daemonDiffBundle]);

  const isAttentionPaneInView = useCallback(
    (detail: { paneId: string; combId: string }) => {
      if (showProviders) return false;
      if (!activeCombId || !activePaneId) return false;
      return detail.combId === activeCombId && detail.paneId === activePaneId;
    },
    [showProviders, activeCombId, activePaneId],
  );

  useTerminalAttentionToasts({
    onNavigateToPane: ({ combId, paneId }) => {
      navigateToComb(combId);
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
      lastAutoSelectedProjectIdRef.current = null;
      return;
    }
    if (activeCombId && combs.some((c) => c.id === activeCombId)) return;
    const stored = localStorage.getItem(`dcc:workspace:${selectedProjectId}:activeComb`);
    if (stored && combs.some((c) => c.id === stored)) {
      setActiveCombId(stored);
      lastAutoSelectedProjectIdRef.current = selectedProjectId;
      return;
    }
    if (lastAutoSelectedProjectIdRef.current === selectedProjectId) return;
    const firstProjectComb = combs.find((c) => c.projectId === selectedProjectId);
    setActiveCombId(firstProjectComb?.id ?? null);
    lastAutoSelectedProjectIdRef.current = selectedProjectId;
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

  const handleSetTheme = useCallback(
    (nextTheme: "dark" | "light" | "system") => {
      setTheme(nextTheme);
    },
    [setTheme],
  );

  const handleToggleTheme = useCallback(() => {
    const nextTheme =
      theme === "dark" ? "light" : theme === "light" ? "system" : "dark";
    setTheme(nextTheme);
  }, [setTheme, theme]);

  const ensureCombWorktree = useCallback(async (combId: string): Promise<string | null> => {
    const prepare = async (): Promise<{ worktreePath: string | null; error: string | null }> => {
      const comb = combsRef.current.find((c) => c.id === combId);
      if (!comb) return { worktreePath: null, error: "Workspace indisponível." };
      const currentPath = comb.worktreePath?.trim();
      if (currentPath) return { worktreePath: currentPath, error: null };

      const api = window.desktopAPI?.comb?.ensureWorktree;
      if (!api) {
        return { worktreePath: null, error: "Preparação de worktree indisponível nesta execução." };
      }

      try {
        const result = await api(combId);
        if (result.success) {
          if (result.warning?.trim()) {
            toast.info(result.warning);
          }
          await refreshCombs();
          const refreshed = combsRef.current.find((c) => c.id === combId);
          const worktreePath = (result.worktreePath ?? refreshed?.worktreePath ?? comb.worktreePath ?? "").trim();
          return { worktreePath: worktreePath || null, error: null };
        }
        return { worktreePath: null, error: result.error ?? "Falha ao preparar worktree" };
      } catch (e: unknown) {
        return { worktreePath: null, error: e instanceof Error ? e.message : "Falha ao preparar worktree" };
      }
    };

    const isStillTarget = () => activeCombIdRef.current === combId && combsRef.current.some((c) => c.id === combId);

    const first = await prepare();
    if (first.worktreePath) return first.worktreePath;
    if (!isStillTarget()) return null;

    await new Promise((resolve) => window.setTimeout(resolve, 350));
    if (!isStillTarget()) return null;

    const second = await prepare();
    if (second.worktreePath) return second.worktreePath;

    toast.error(second.error ?? first.error ?? "Falha ao preparar worktree");
    return null;
  }, [refreshCombs]);

  const prepareWorkspace = useCallback(async (combId: string, task: () => Promise<void>) => {
    setWorkspacePrepCombId(combId);
    try {
      await task();
    } finally {
      setWorkspacePrepCombId((current) => (current === combId ? null : current));
    }
  }, []);

  const handleSelectWorkspace = useCallback(
    (comb: Comb) => {
      navigateToComb(comb.id);
      setShowProviders(false);
    },
    [navigateToComb],
  );

  const handleOpenReview = useCallback(() => {
    if (!activeCombId) return;
    setWorkspaceView("review");
  }, [activeCombId]);

  const handleOpenPanes = useCallback(() => {
    setWorkspaceView("panes");
  }, []);

  // ── Git sidebar logic ────────────────────────────────────────────
  const activeCombWorktreePath = activeComb?.worktreePath?.trim() ?? "";
  const activeCombMainPath = activeProject?.path ?? "";

  const loadGitSidebarDiffs = useCallback(async (worktreePath: string) => {
    if (!worktreePath || !window.desktopAPI?.git?.getReviewDiffs) return;
    setGitSidebarLoading(true);
    try {
      const result = await window.desktopAPI.git.getReviewDiffs(worktreePath);
      if (result.success) {
        setGitSidebarFiles(result.files ?? []);
      } else {
        setGitSidebarFiles([]);
      }
    } catch {
      setGitSidebarFiles([]);
    } finally {
      setGitSidebarLoading(false);
    }
  }, []);

  const refreshGitSidebarStatus = useCallback(async (worktreePath: string, mainPath: string) => {
    const git = window.desktopAPI?.git;
    if (!git?.getStatus) return;
    const [wt, main] = await Promise.allSettled([
      worktreePath ? git.getStatus(worktreePath) : Promise.resolve(null),
      mainPath ? git.getStatus(mainPath) : Promise.resolve(null),
    ]);
    setGitSidebarWorktreeDirty(wt.status === "fulfilled" && (wt.value?.isDirty ?? false));
    setGitSidebarMainDirty(main.status === "fulfilled" && (main.value?.isDirty ?? false));
  }, []);

  useEffect(() => {
    if (!activeCombWorktreePath) {
      setGitSidebarFiles([]);
      setGitSidebarBranchList([]);
      setGitSidebarTargetBranch("");
      return;
    }
    void loadGitSidebarDiffs(activeCombWorktreePath);
    void refreshGitSidebarStatus(activeCombWorktreePath, activeCombMainPath);
    const git = window.desktopAPI?.git;
    if (!activeCombMainPath || !git?.getLocalBranches || !git?.getCurrentBranch) return;
    Promise.all([git.getLocalBranches(activeCombMainPath), git.getCurrentBranch(activeCombMainPath)])
      .then(([branches, current]) => {
        const list = branches ?? [];
        setGitSidebarBranchList(list);
        setGitSidebarTargetBranch((prev) => prev || (current ?? "").trim() || list[0] || "main");
      })
      .catch(() => undefined);
  }, [activeCombWorktreePath, activeCombMainPath, loadGitSidebarDiffs, refreshGitSidebarStatus]);

  useEffect(() => {
    if (!gitSidebarCommitOpen) {
      setGitSidebarCommitStatus(null);
      setGitSidebarCommitStatusLoading(false);
      return;
    }
    if (!activeCombWorktreePath || !window.desktopAPI?.git?.getStatus) return;
    setGitSidebarCommitStatusLoading(true);
    window.desktopAPI.git
      .getStatus(activeCombWorktreePath)
      .then((s) => setGitSidebarCommitStatus(s))
      .catch(() => {})
      .finally(() => setGitSidebarCommitStatusLoading(false));
  }, [gitSidebarCommitOpen, activeCombWorktreePath]);

  const handleGitSidebarCommit = useCallback(async (message: string) => {
    if (!activeCombWorktreePath || !window.desktopAPI?.git?.commit) return;
    const result = await window.desktopAPI.git.commit(activeCombWorktreePath, message);
    if (result.success) {
      toast.success("Commit realizado");
      void loadGitSidebarDiffs(activeCombWorktreePath);
      void refreshGitSidebarStatus(activeCombWorktreePath, activeCombMainPath);
      await refreshCombs({ silent: true });
    } else {
      toast.error(result.error ?? "Falha no commit");
      throw new Error(result.error ?? "Falha no commit");
    }
  }, [activeCombWorktreePath, activeCombMainPath, loadGitSidebarDiffs, refreshGitSidebarStatus, refreshCombs]);

  const handleGitSidebarPush = useCallback(async () => {
    if (!activeCombWorktreePath || !window.desktopAPI?.git?.push) return;
    setGitSidebarIsPushing(true);
    try {
      const result = await window.desktopAPI.git.push(activeCombWorktreePath);
      if (result?.success) {
        toast.success("Push enviado");
        void loadGitSidebarDiffs(activeCombWorktreePath);
      } else {
        toast.error(result?.error ?? "Falha ao fazer push");
      }
    } finally {
      setGitSidebarIsPushing(false);
    }
  }, [activeCombWorktreePath, loadGitSidebarDiffs]);

  const handleGitSidebarPull = useCallback(async () => {
    if (!activeCombWorktreePath || !window.desktopAPI?.git?.pull) return;
    setGitSidebarIsPulling(true);
    try {
      const result = await window.desktopAPI.git.pull(activeCombWorktreePath);
      if (result?.success) {
        toast.success("Pull concluído");
        void loadGitSidebarDiffs(activeCombWorktreePath);
        void refreshGitSidebarStatus(activeCombWorktreePath, activeCombMainPath);
      } else {
        toast.error(result?.error ?? "Falha ao fazer pull");
      }
    } finally {
      setGitSidebarIsPulling(false);
    }
  }, [activeCombWorktreePath, activeCombMainPath, loadGitSidebarDiffs, refreshGitSidebarStatus]);

  const handleGitSidebarDiscard = useCallback(async () => {
    if (!activeCombWorktreePath || !window.desktopAPI?.git?.reset) return;
    const ok = await confirmDialog({
      title: "Descartar alterações locais?",
      description: "Será executado git reset --hard neste worktree.",
      confirmLabel: "Descartar",
      cancelLabel: "Cancelar",
    });
    if (!ok) return;
    const result = await window.desktopAPI.git.reset(activeCombWorktreePath, "HEAD");
    if (result.success) {
      toast.success("Alterações descartadas");
      void loadGitSidebarDiffs(activeCombWorktreePath);
      void refreshGitSidebarStatus(activeCombWorktreePath, activeCombMainPath);
    } else {
      toast.error(result.error ?? "Falha ao descartar");
    }
  }, [activeCombWorktreePath, activeCombMainPath, confirmDialog, loadGitSidebarDiffs, refreshGitSidebarStatus]);

  const handleGitSidebarMerge = useCallback(async () => {
    if (!activeComb?.id || !gitSidebarTargetBranch) return;
    setGitSidebarIsMerging(true);
    try {
      const result = await window.desktopAPI?.comb?.mergeIntoMain(activeComb.id, gitSidebarTargetBranch);
      if (result?.success) {
        toast.success("Merge concluído");
        setGitSidebarMergeOpen(false);
        void loadGitSidebarDiffs(activeCombWorktreePath);
        void refreshGitSidebarStatus(activeCombWorktreePath, activeCombMainPath);
        await refreshCombs({ silent: true });
      } else {
        toast.error(result?.error ?? "Falha ao fazer merge");
      }
    } finally {
      setGitSidebarIsMerging(false);
    }
  }, [activeComb, gitSidebarTargetBranch, activeCombWorktreePath, activeCombMainPath, loadGitSidebarDiffs, refreshGitSidebarStatus, refreshCombs]);

  const canMergeGitSidebar = Boolean(
    activeComb?.id && activeCombWorktreePath && window.desktopAPI?.comb?.mergeIntoMain,
  );
  const mergeGitSidebarBlocked =
    !canMergeGitSidebar || gitSidebarWorktreeDirty || gitSidebarMainDirty || gitSidebarIsMerging;

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

  useEffect(() => {
    const handleGoToPanes = () => setWorkspaceView("panes");
    window.addEventListener("dcc:hive:goto-panes", handleGoToPanes);
    return () => {
      window.removeEventListener("dcc:hive:goto-panes", handleGoToPanes);
    };
  }, []);

  const handleAddTerminal = async () => {
    const combId = activeCombIdRef.current;
    if (!combId) return;
    if (worktreePrepInFlightRef.current === combId) return;
    worktreePrepInFlightRef.current = combId;
    try {
      await prepareWorkspace(combId, async () => {
        const worktreePath = await ensureCombWorktree(combId);
        if (!worktreePath || activeCombIdRef.current !== combId) return;
        const pane = await createPane({ combId, type: "term" });
        await updatePane(pane.id, { cwd: worktreePath });
        await refreshPanes();
        setActivePaneId(pane.id);
      });
    } catch {
      toast.error("Falha ao abrir terminal");
    } finally {
      if (worktreePrepInFlightRef.current === combId) {
        worktreePrepInFlightRef.current = null;
      }
    }
  };

  const launchManagedCommand = useCallback(
    async (payload: {
      title: string;
      command: string;
      cwdMode?: "project" | "worktree";
      description?: string | null;
    }) => {
      const combId = activeCombIdRef.current;
      if (!combId) return;
      const cwdMode = payload.cwdMode ?? "worktree";
      const projectPath = activeProject?.path?.trim() ?? "";
      let cwd = projectPath;
      if (cwdMode === "worktree") {
        let worktreePath: string | null = null;
        await prepareWorkspace(combId, async () => {
          worktreePath = await ensureCombWorktree(combId);
        });
        if (!worktreePath) {
          toast.error("Worktree indisponível para este workspace.");
          return;
        }
        cwd = worktreePath;
      } else if (!projectPath) {
        toast.error("Caminho do projeto indisponível.");
        return;
      }

      if (activeCombIdRef.current !== combId) return;

      try {
        const pane = await createPane({
          combId,
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
    [activeProject?.path, createPane, ensureCombWorktree, refreshPanes, updatePane],
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
    setWorkspaceRemovalDialog({
      combId,
      title: dialogCopy.title,
      description: dialogCopy.description,
      confirmLabel: dialogCopy.confirmLabel,
      confirmVariant: dialogCopy.confirmVariant,
      isRemoving: false,
    });
  };
  const handleRemoveWorkspaceById = useCallback((combId: string) => {
    void handleRemoveWorkspace(combId);
  }, [handleRemoveWorkspace]);

  const handleConfirmRemoveWorkspace = useCallback(async () => {
    const dialog = workspaceRemovalDialog;
    if (!dialog || dialog.isRemoving) return;
    setWorkspaceRemovalDialog((current) => (current ? { ...current, isRemoving: true } : current));
    const { combId } = dialog;

    try {
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
      if (activeCombId === combId) {
        setActiveCombId(null);
        setActivePaneId(null);
        lastAutoSelectedProjectIdRef.current = selectedProjectId ?? null;
      }
      refreshCombs();
      setAttentionRecords((prev) => prev.filter((item) => item.combId !== combId));
      toast.success("Workspace removido");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Falha ao remover workspace");
    } finally {
      setWorkspaceRemovalDialog(null);
    }
  }, [activeCombId, refreshCombs, selectedProjectId, workspaceRemovalDialog]);
  const handleRemovePaneById = useCallback((paneId: string) => {
    void handleRemovePane(paneId);
  }, [handleRemovePane]);

  const handleRenamePane = useCallback(
    async (paneId: string, newTitle: string) => {
      try {
        await updatePane(paneId, { title: newTitle });
        toast.success("Tab renomeado");
      } catch (error) {
        toast.error("Falha ao renomear tab");
        console.error("Error renaming pane:", error);
      }
    },
    [updatePane],
  );

  const handleDragEnd = useCallback(
    async (result: DropResult) => {
      if (!result.destination) return;

      const sourceIndex = result.source.index;
      const destIndex = result.destination.index;

      if (sourceIndex === destIndex) return;

      // Reordenar array
      const reordered = Array.from(visiblePanes);
      const [movedPane] = reordered.splice(sourceIndex, 1);
      reordered.splice(destIndex, 0, movedPane);

      // Atualizar layout_order no DB
      try {
        const updates = reordered.map((pane, index) =>
          updatePane(pane.id, { layoutOrder: index })
        );
        await Promise.all(updates);
        toast.success("Tabs reordenados");
      } catch (error) {
        toast.error("Falha ao reordenar tabs");
        console.error("Error reordering panes:", error);
      }
    },
    [visiblePanes, updatePane],
  );

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

  useEffect(() => {
    const isModifierPressed = (event: KeyboardEvent) =>
      isMacPlatform ? event.metaKey : event.ctrlKey;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (isModifierPressed(event)) {
        setShowShortcutHints(true);
      }
    };

    const handleKeyUp = (event: KeyboardEvent) => {
      if (
        event.key === "Meta" ||
        event.key === "Control" ||
        !isModifierPressed(event)
      ) {
        setShowShortcutHints(false);
      }
    };

    const handleBlur = () => setShowShortcutHints(false);

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("blur", handleBlur);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("blur", handleBlur);
    };
  }, [isMacPlatform]);

  /**
   * Atalhos estilo Maestro: Cmd+1–9 (foco), Cmd+K (palette / limpar), zoom e comandos rápidos.
   * Inclui ações de UI/UX: notificações, tema e comandos do workspace.
   * Ignora foco em dialog/input (exceto textarea do xterm).
   * Conflitos possíveis: Cmd+K noutras apps; zoom do browser — aqui preventDefault no workspace.
   */
  useEffect(() => {
    const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/i.test(navigator.platform);
    const mod = isMac ? (e: KeyboardEvent) => e.metaKey : (e: KeyboardEvent) => e.ctrlKey;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat) return;

      if (mod(event) && (event.key === "k" || event.key === "K")) {
        event.preventDefault();
        if (event.shiftKey) {
          window.dispatchEvent(new CustomEvent("dcc-terminal-action", { detail: { type: "clearScrollback" } }));
        } else {
          setCommandPaletteOpen(true);
        }
        return;
      }

      const el = event.target;
      if (el instanceof HTMLElement) {
        const inXterm = el.closest(".xterm");
        const inDialog = el.closest("[role=\"dialog\"], [data-radix-dialog-content]");
        if (inDialog && !inXterm) return;
        if (el.closest("input, textarea, select, [contenteditable='true']") && !inXterm) return;
      }

      if (mod(event) && !event.shiftKey && (event.key === "[" || event.key === "]")) {
        event.preventDefault();
        if (event.key === "[") {
          goWorktreeBack();
        } else {
          goWorktreeForward();
        }
        return;
      }

      if (!mod(event)) return;

      if (event.shiftKey) {
        const key = event.key.toLowerCase();
        if (key === "n") {
          event.preventDefault();
          setNewCombOpen(true);
          return;
        }
        if (key === "t") {
          event.preventDefault();
          void handleAddTerminal();
          return;
        }
        if (key === "a") {
          event.preventDefault();
          if (!activeCombId) return;
          setNewAgentOpen(true);
          return;
        }
        if (key === "b") {
          event.preventDefault();
          void handleOpenBaseTerminal();
          return;
        }
        if (key === "r") {
          event.preventDefault();
          setRepoConfigOpen(true);
          return;
        }
        if (key === "v") {
          event.preventDefault();
          handleOpenReview();
          return;
        }
        if (key === "i") {
          event.preventDefault();
          setAttentionOpen(true);
          return;
        }
        if (key === "p") {
          event.preventDefault();
          setShowProviders((prev) => !prev);
          return;
        }
        if (key === "d") {
          event.preventDefault();
          handleSetTheme("dark");
          return;
        }
        if (key === "l") {
          event.preventDefault();
          handleSetTheme("light");
          return;
        }
        if (key === "s") {
          event.preventDefault();
          handleSetTheme("system");
          return;
        }
      }

      if (event.altKey && (event.key === "t" || event.key === "T")) {
        event.preventDefault();
        handleToggleTheme();
        return;
      }

      if (!activeCombId) return;

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
    goWorktreeBack,
    goWorktreeForward,
    handleAddTerminal,
    handleOpenBaseTerminal,
    handleOpenReview,
    handleSetTheme,
    handleToggleTheme,
    markPaneAttentionAsRead,
    visiblePanes,
  ]);

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
          <Button
            variant="outline"
            className="titlebar-no-drag w-full justify-start gap-2"
            onClick={() => setAddProjectOpen(true)}
          >
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
                  const agents = activeAgentsByCombId.get(comb.id) ?? [];
                  const reviewSummary = comb.worktreePath?.trim()
                    ? (reviewSummaryByWorktreePath.get(comb.worktreePath.trim()) ?? null)
                    : null;
                  return (
                    <WorkspaceListItem
                      key={comb.id}
                      comb={comb}
                      isActive={isActive}
                      projectName={projectName}
                      hasAttention={!!attentionForComb}
                      reviewSummary={reviewSummary}
                      runningCount={runningCount}
                      agentCount={agents.length}
                      agentPreview={agents.slice(0, 2)}
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
              {showShortcutHints ? (
                <Kbd className="ml-auto h-5 px-1.5 text-[10px] font-medium">
                  {isMacPlatform ? "⌘⇧I" : "Ctrl+Shift+I"}
                </Kbd>
              ) : null}
              {unreadCount > 0 ? <Badge className="ml-1">{unreadCount}</Badge> : null}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="titlebar-no-drag w-full justify-start gap-2"
              onClick={() => setShowProviders((prev) => !prev)}
            >
              <Settings className="h-4 w-4" />
              Providers
              {showShortcutHints ? (
                <Kbd className="ml-auto h-5 px-1.5 text-[10px] font-medium">
                  {isMacPlatform ? "⌘⇧P" : "Ctrl+Shift+P"}
                </Kbd>
              ) : null}
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="titlebar-no-drag w-full justify-start gap-2"
              onClick={() => setCommandPaletteOpen(true)}
            >
              <WandSparkles className="h-4 w-4" />
              Palette
              {showShortcutHints ? (
                <Kbd className="ml-auto h-5 px-1.5 text-[10px] font-medium">
                  {isMacPlatform ? "⌘K" : "Ctrl+K"}
                </Kbd>
              ) : null}
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="titlebar-no-drag w-full justify-start gap-2"
              onClick={() => setRepoConfigOpen(true)}
            >
              <Settings className="h-4 w-4" />
              Repo
              {showShortcutHints ? (
                <Kbd className="ml-auto h-5 px-1.5 text-[10px] font-medium">
                  {isMacPlatform ? "⌘⇧R" : "Ctrl+Shift+R"}
                </Kbd>
              ) : null}
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
                <p className="mt-1 text-[10px] text-sidebar-foreground/45">
                  {isMacPlatform
                    ? "Segure ⌘ para revelar atalhos."
                    : "Segure Ctrl para revelar atalhos."}
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

                {projectActivity.activeAgents?.length ? (
                  <div className="rounded-md border border-sidebar-border/70 bg-sidebar-accent/25 px-3 py-2">
                    <div className="flex items-center justify-between gap-2">
                      <p className="text-[10px] font-semibold uppercase tracking-wide text-sidebar-foreground/55">
                        Agentes detectados
                      </p>
                      <div className="flex items-center gap-1">
                        <Badge variant="secondary" className="h-5 px-1.5 text-[10px]">
                          {projectActivity.workingAgents ?? 0} working
                        </Badge>
                        <Badge variant="destructive" className="h-5 px-1.5 text-[10px]">
                          {projectActivity.waitingAgents ?? 0} waiting
                        </Badge>
                      </div>
                    </div>
                    <div className="mt-2 space-y-1">
                      {projectActivity.activeAgents.slice(0, 4).map((agent) => (
                        <div
                          key={agent.ptyId}
                          className="flex items-start justify-between gap-2 rounded border border-sidebar-border/60 bg-sidebar-accent/40 px-2 py-1"
                        >
                          <div className="min-w-0">
                            <p className="truncate text-[11px] font-medium">
                              {agent.agentLabel}
                              <span className="ml-1 text-[10px] font-normal text-sidebar-foreground/55">
                                {agent.workspaceName ?? agent.projectName}
                              </span>
                            </p>
                            {agent.title ? (
                              <p className="truncate text-[10px] text-sidebar-foreground/50">
                                {agent.title}
                              </p>
                            ) : null}
                            <p className="truncate text-[10px] text-sidebar-foreground/50">
                              {agent.cwd}
                            </p>
                          </div>
                          <Badge
                            variant={agent.status === "waiting" ? "destructive" : "secondary"}
                            className="shrink-0 h-5 px-1.5 text-[10px]"
                          >
                            {agent.status}
                          </Badge>
                        </div>
                      ))}
                      {projectActivity.activeAgents.length > 4 ? (
                        <p className="text-[10px] text-sidebar-foreground/45">
                          +{projectActivity.activeAgents.length - 4} agentes adicionais.
                        </p>
                      ) : null}
                    </div>
                  </div>
                ) : null}

                <div className="rounded-md border border-sidebar-border/70 bg-sidebar-accent/30 px-3 py-2">
                  <div className="flex items-center gap-2 text-xs font-medium">
                    <Clock3 className="h-3.5 w-3.5" />
                    Daemon local
                  </div>
                  <p className="mt-1 text-[11px] text-sidebar-foreground/60">
                    {daemonStatus?.running ? "Executando" : "Parado"}
                    {daemonStatus?.lastTickAt ? (
                      <>
                        {" "}
                        · atualizado em {formatDaemonLastTick(daemonStatus.lastTickAt)}
                      </>
                    ) : null}
                  </p>
                  <p className="mt-1 text-[11px] text-sidebar-foreground/70">
                    {daemonStatus?.runningTasks ?? 0} em execução ·{" "}
                    {daemonStatus?.enabledTasks ?? activeRepoTasks.length} tarefas habilitadas
                  </p>
                  <div className="mt-2 grid grid-cols-2 gap-2">
                    <div className="rounded-md border border-sidebar-border/60 bg-sidebar-accent/40 px-2 py-1.5">
                      <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wide text-sidebar-foreground/50">
                        <Cpu className="h-3 w-3" />
                        CPU
                      </div>
                      <p className="mt-1 text-xs font-semibold text-sidebar-foreground/90">
                        {typeof daemonStatus?.cpuPercent === "number"
                          ? `${daemonStatus.cpuPercent.toFixed(1)}%`
                          : "—"}
                      </p>
                    </div>
                    <div className="rounded-md border border-sidebar-border/60 bg-sidebar-accent/40 px-2 py-1.5">
                      <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wide text-sidebar-foreground/50">
                        <MemoryStick className="h-3 w-3" />
                        RAM
                      </div>
                      <p className="mt-1 text-xs font-semibold text-sidebar-foreground/90">
                        {typeof daemonStatus?.memoryMb === "number"
                          ? formatDaemonMemory(daemonStatus.memoryMb)
                          : "—"}
                      </p>
                    </div>
                  </div>
                  {daemonStatus?.lastMetricsAt ? (
                    <p className="mt-2 text-[10px] text-sidebar-foreground/45">
                      Métricas atualizadas{" "}
                      {formatDistanceToNow(new Date(daemonStatus.lastMetricsAt), {
                        locale: ptBR,
                        addSuffix: true,
                      })}
                    </p>
                  ) : null}
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
                                  navigateToComb(comb.id);
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
                                      navigateToComb(comb.id);
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

      <main className="relative flex min-h-0 flex-1 flex-col overflow-hidden" data-workspace-main>
        {showProviders ? (
          <div className="flex-1 overflow-auto mt-8">
            <SettingsPage />
          </div>
        ) : activeComb ? (
          <>
            <div className="mt-8 flex items-center justify-between border-b border-border px-4 py-2">
              <div className="flex min-w-0 items-center gap-2 sm:gap-3">
                <div className="flex shrink-0 items-center gap-0.5">
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="titlebar-no-drag h-8 w-8 shrink-0"
                        disabled={!canGoWorktreeBack}
                        aria-label="Workspace anterior"
                        onClick={() => goWorktreeBack()}
                      >
                        <ChevronLeft className="h-4 w-4" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                      <span className="inline-flex items-center gap-2">
                        Workspace anterior
                        {showShortcutHints ? (
                          <Kbd className="h-5 px-1.5 text-[10px] font-medium">
                            {isMacPlatform ? "⌘[" : "Ctrl+["}
                          </Kbd>
                        ) : null}
                      </span>
                    </TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="titlebar-no-drag h-8 w-8 shrink-0"
                        disabled={!canGoWorktreeForward}
                        aria-label="Workspace seguinte"
                        onClick={() => goWorktreeForward()}
                      >
                        <ChevronRight className="h-4 w-4" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                      <span className="inline-flex items-center gap-2">
                        Workspace seguinte
                        {showShortcutHints ? (
                          <Kbd className="h-5 px-1.5 text-[10px] font-medium">
                            {isMacPlatform ? "⌘]" : "Ctrl+]"}
                          </Kbd>
                        ) : null}
                      </span>
                    </TooltipContent>
                  </Tooltip>
                </div>
                <h3 className="truncate text-sm font-semibold">{activeComb.name}</h3>
                <div className="flex flex-wrap items-center gap-1.5">
                  <Badge variant="outline" className="gap-1">
                    <GitBranch className="h-3 w-3" />
                    {activeComb.branch ?? activeComb.baseBranch}
                  </Badge>
                  {activeComb.worktreePath?.trim() ? (
                    <Badge
                      variant="secondary"
                      className="gap-1"
                      title="Resumo do review deste workspace"
                    >
                      <GitPullRequest className="h-3 w-3" />
                      {(() => {
                        const summary = reviewSummaryByWorktreePath.get(activeComb.worktreePath!.trim());
                        if (!summary || summary.changedFiles <= 0) return "Review";
                        return `Review · ${summary.changedFiles} +${summary.insertions}/-${summary.deletions}`;
                      })()}
                    </Badge>
                  ) : null}
                </div>
              </div>
              <div className="flex items-center gap-2">
                <Tooltip>
                  <TooltipTrigger asChild>
            <Button variant="outline" size="sm" onClick={handleOpenBaseTerminal}>
              <FolderGit2 className="mr-1 h-3.5 w-3.5" />
              Base
              {showShortcutHints ? (
                <Kbd className="ml-2 h-5 px-1.5 text-[10px] font-medium">
                  {isMacPlatform ? "⌘⇧B" : "Ctrl+Shift+B"}
                </Kbd>
              ) : null}
            </Button>
          </TooltipTrigger>
          <TooltipContent>Terminal no repositorio principal</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="outline" size="sm" onClick={handleAddTerminal}>
              <Terminal className="mr-1 h-3.5 w-3.5" />
              Workspace
              {showShortcutHints ? (
                <Kbd className="ml-2 h-5 px-1.5 text-[10px] font-medium">
                  {isMacPlatform ? "⌘⇧T" : "Ctrl+Shift+T"}
                </Kbd>
              ) : null}
            </Button>
          </TooltipTrigger>
          <TooltipContent>Terminal no worktree atual</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="outline" size="sm" onClick={() => setNewAgentOpen(true)} disabled={cliProviders.length === 0}>
              <Bot className="mr-1 h-3.5 w-3.5" />
              Agent
              {showShortcutHints ? (
                <Kbd className="ml-2 h-5 px-1.5 text-[10px] font-medium">
                  {isMacPlatform ? "⌘⇧A" : "Ctrl+Shift+A"}
                </Kbd>
              ) : null}
            </Button>
          </TooltipTrigger>
          <TooltipContent>Abrir agente CLI no workspace</TooltipContent>
        </Tooltip>
              </div>
            </div>
            <div className="relative min-h-0 flex-1 overflow-hidden">
              {workspacePrepCombId === activeCombId ? (
                <div className="absolute inset-0 z-20 flex items-center justify-center bg-background/75 backdrop-blur-sm">
                  <div className="flex flex-col items-center gap-3 rounded-xl border border-border bg-background/90 px-6 py-5 shadow-lg">
                    <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
                    <div className="text-center">
                      <p className="text-sm font-medium">Preparando workspace</p>
                      <p className="text-xs text-muted-foreground">A criar ou reidratar o worktree...</p>
                    </div>
                  </div>
                </div>
              ) : null}
              <ResizablePanelGroup direction="horizontal" className="h-full min-h-0">
                <ResizablePanel defaultSize={75} minSize={40} className="min-h-0 overflow-hidden">
              <div className="flex h-full min-h-0 flex-col overflow-hidden">
                {panesLoading ? (
                  <div className="flex h-full flex-col items-center justify-center gap-3 p-8">
                    <Loader2 className="h-10 w-10 animate-spin text-muted-foreground/35" />
                    <p className="text-sm text-muted-foreground">A abrir workspace…</p>
                  </div>
                ) : visiblePanes.length === 0 && !activeDiffPath ? (
                  <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
                    <Terminal className="h-12 w-12 text-muted-foreground/30" />
                    <p className="text-sm text-muted-foreground">Nenhum pane aberto neste workspace</p>
                  </div>
                ) : (
                  <div className="flex h-full min-h-0 flex-col overflow-hidden p-1">
                    <DragDropContext onDragEnd={handleDragEnd}>
                      <Droppable droppableId="pane-tabs" direction="horizontal">
                        {(provided) => (
                          <div
                            role="tablist"
                            aria-label="Panes do workspace"
                            className="mb-1 flex shrink-0 gap-1 overflow-x-auto"
                            ref={provided.innerRef}
                            {...provided.droppableProps}
                          >
                            {visiblePanes.map((pane, index) => {
                              const provider = pane.providerId ? (providerById.get(pane.providerId) ?? null) : null;
                              const label = pane.type === "agent" ? (pane.title ?? provider?.name ?? "Agent") : (pane.title ?? "Terminal");
                              const selected = pane.id === activePane?.id && !diffTabActive;
                              const hasUnreadAttention = hasUnreadAttentionByPaneId.has(pane.id);
                              return (
                                <Draggable key={pane.id} draggableId={pane.id} index={index}>
                                  {(provided, snapshot) => (
                                    <div ref={provided.innerRef} {...provided.draggableProps}>
                                      <PaneTab
                                        pane={pane}
                                        provider={provider}
                                        selected={selected}
                                        hasUnreadAttention={hasUnreadAttention}
                                        label={label}
                                        onSelect={(id) => {
                                          setDiffTabActive(false);
                                          handleSelectPaneTab(id);
                                        }}
                                        onRemove={handleRemovePaneById}
                                        onRename={handleRenamePane}
                                        isDragging={snapshot.isDragging}
                                        dragHandleProps={provided.dragHandleProps}
                                      />
                                    </div>
                                  )}
                                </Draggable>
                              );
                            })}
                            {provided.placeholder}
                            {/* Virtual diff tab */}
                            {activeDiffPath && (() => {
                              const diffFile = gitSidebarFiles.find(f => f.path === activeDiffPath);
                              const diffBasename = activeDiffPath.split(/[/\\]/).pop() ?? activeDiffPath;
                              return (
                                <div
                                  role="tab"
                                  aria-selected={diffTabActive}
                                  onClick={() => setDiffTabActive(true)}
                                  className={`group flex min-w-[170px] max-w-[260px] cursor-pointer items-center gap-2 rounded-md border px-2 py-1.5 transition-all ${
                                    diffTabActive
                                      ? "border-primary bg-primary/10"
                                      : "border-border bg-muted/30 hover:bg-muted/50"
                                  }`}
                                >
                                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
                                    <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
                                    <polyline points="14 2 14 8 20 8"/>
                                    <line x1="9" y1="12" x2="15" y2="12"/>
                                  </svg>
                                  <span className="min-w-0 flex-1 truncate font-mono text-xs">{diffBasename}</span>
                                  {diffFile && (
                                    <span className={`shrink-0 rounded px-1 py-0.5 text-[9px] font-bold uppercase ${
                                      diffFile.status.charAt(0).toUpperCase() === "A" ? "bg-emerald-500/20 text-emerald-600 dark:text-emerald-400" :
                                      diffFile.status.charAt(0).toUpperCase() === "M" ? "bg-amber-500/20 text-amber-600 dark:text-amber-400" :
                                      diffFile.status.charAt(0).toUpperCase() === "D" ? "bg-rose-500/20 text-rose-600 dark:text-rose-400" :
                                      "bg-blue-500/20 text-blue-600 dark:text-blue-400"
                                    }`}>
                                      {diffFile.status.charAt(0).toUpperCase()}
                                    </span>
                                  )}
                                  <button
                                    className="ml-auto h-5 w-5 shrink-0 flex items-center justify-center rounded opacity-70 hover:opacity-100 hover:bg-muted"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      setActiveDiffPath(null);
                                      setDiffTabActive(false);
                                    }}
                                  >
                                    <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                      <line x1="18" y1="6" x2="6" y2="18"/>
                                      <line x1="6" y1="6" x2="18" y2="18"/>
                                    </svg>
                                  </button>
                                </div>
                              );
                            })()}
                          </div>
                        )}
                      </Droppable>
                    </DragDropContext>
                    <div className="min-h-0 flex-1 overflow-hidden">
                      {diffTabActive && activeDiffPath ? (
                        (() => {
                          const diffFile = gitSidebarFiles.find(f => f.path === activeDiffPath);
                          return diffFile ? (
                            <DiffViewer
                              file={diffFile}
                              onClose={() => {
                                setActiveDiffPath(null);
                                setDiffTabActive(false);
                              }}
                            />
                          ) : null;
                        })()
                      ) : activePane ? (
                        <PaneCard
                          pane={activePane}
                          worktreePath={activePane.cwd?.trim() || activeComb.worktreePath || ""}
                          provider={activePane.providerId ? (providerById.get(activePane.providerId) ?? null) : null}
                          combId={activeComb.id}
                          projectId={activeComb.projectId}
                          onPaneStatusChange={handlePaneStatusChange}
                          onRemovePane={handleRemovePaneById}
                        />
                      ) : null}
                    </div>
                  </div>
                )}
              </div>
                </ResizablePanel>

                <ResizableHandle withHandle className="bg-border/70" />

                {/* ── Git Actions sidebar (always visible) ── */}
                <ResizablePanel
                  defaultSize={25}
                  minSize={18}
                  maxSize={42}
                  className="min-h-0 overflow-hidden"
                >
                  {activeCombWorktreePath ? (
                    <GitActionsPanel
                      files={gitSidebarLoading ? [] : gitSidebarFiles}
                      fileFlags={{}}
                      activeDiffPath={activeDiffPath}
                      onFileSelect={(path) => {
                        setActiveDiffPath(path);
                        setDiffTabActive(true);
                      }}
                      isPushing={gitSidebarIsPushing}
                      isPulling={gitSidebarIsPulling}
                      isMerging={gitSidebarIsMerging}
                      worktreeDirty={gitSidebarWorktreeDirty}
                      mainRepoDirty={gitSidebarMainDirty}
                      canMergeComb={canMergeGitSidebar}
                      mergeUiBlocked={mergeGitSidebarBlocked}
                      onCommit={() => setGitSidebarCommitOpen(true)}
                      onPush={() => void handleGitSidebarPush()}
                      onPull={() => void handleGitSidebarPull()}
                      onDiscard={() => void handleGitSidebarDiscard()}
                      onMerge={() => setGitSidebarMergeOpen(true)}
                      targetBranch={gitSidebarTargetBranch}
                      branchList={gitSidebarBranchList}
                      onTargetBranchChange={setGitSidebarTargetBranch}
                    />
                  ) : (
                    <div className="flex h-full flex-col items-center justify-center gap-2 p-4">
                      <p className="text-center text-xs font-medium text-muted-foreground">
                        Git review não disponível
                      </p>
                      <p className="text-center text-xs text-muted-foreground/70">
                        Use o <span className="font-semibold text-muted-foreground">Workspace Terminal</span> para ativar o git review com isolamento de worktree.
                      </p>
                    </div>
                  )}
                </ResizablePanel>
              </ResizablePanelGroup>

              {/* CommitDialog + MergeDialog for sidebar */}
              <CommitDialog
                open={gitSidebarCommitOpen}
                onOpenChange={setGitSidebarCommitOpen}
                onCommit={handleGitSidebarCommit}
                defaultMessage={activeComb ? `Changes from mission: ${activeComb.branch ?? activeComb.id}` : ""}
                projectPath={activeCombWorktreePath}
                status={gitSidebarCommitStatus}
                isLoading={gitSidebarCommitStatusLoading}
              />
              <Dialog open={gitSidebarMergeOpen} onOpenChange={setGitSidebarMergeOpen}>
                <DialogContent className="sm:max-w-sm">
                  <DialogHeader>
                    <DialogTitle>Merge na branch de destino</DialogTitle>
                    <DialogDescription>
                      Integra o branch da Missão em{" "}
                      <span className="font-mono">{gitSidebarTargetBranch || "…"}</span>.
                    </DialogDescription>
                  </DialogHeader>
                  <DialogFooter>
                    <Button variant="outline" onClick={() => setGitSidebarMergeOpen(false)}>
                      Cancelar
                    </Button>
                    <Button
                      onClick={() => void handleGitSidebarMerge()}
                      disabled={gitSidebarIsMerging || mergeGitSidebarBlocked}
                    >
                      {gitSidebarIsMerging && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                      Merge
                    </Button>
                  </DialogFooter>
                </DialogContent>
              </Dialog>
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
                    navigateToComb(item.combId);
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
          navigateToComb(comb.id);
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
          ensureCombWorktree={ensureCombWorktree}
          prepareWorkspace={prepareWorkspace}
          updatePane={updatePane}
          onCreate={async (pane) => {
            await refreshPanes();
            setActivePaneId(pane.id);
          }}
        />
      ) : null}

      <Dialog
        open={workspaceRemovalDialog !== null}
        onOpenChange={(open) => {
          if (!open && !workspaceRemovalDialog?.isRemoving) {
            setWorkspaceRemovalDialog(null);
          }
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{workspaceRemovalDialog?.title ?? "Remover workspace?"}</DialogTitle>
            {workspaceRemovalDialog?.description ? (
              <DialogDescription>{workspaceRemovalDialog.description}</DialogDescription>
            ) : null}
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setWorkspaceRemovalDialog(null)}
              disabled={workspaceRemovalDialog?.isRemoving}
            >
              Cancelar
            </Button>
            <Button
              variant={workspaceRemovalDialog?.confirmVariant === "destructive" ? "destructive" : "default"}
              onClick={() => void handleConfirmRemoveWorkspace()}
              disabled={!workspaceRemovalDialog || workspaceRemovalDialog.isRemoving}
            >
              {workspaceRemovalDialog?.isRemoving ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : null}
              {workspaceRemovalDialog?.isRemoving
                ? "Removendo..."
                : workspaceRemovalDialog?.confirmLabel ?? "Remover"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

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
        onOpenReview={handleOpenReview}
        onOpenRepoConfig={() => setRepoConfigOpen(true)}
        onOpenNotifications={() => setAttentionOpen(true)}
        currentTheme={theme}
        onSetTheme={handleSetTheme}
        onToggleTheme={handleToggleTheme}
        onSelectProject={(projectId) => {
          setSelectedProjectId(projectId);
          const nextProjectComb = combs.find((comb) => comb.projectId === projectId) ?? null;
          navigateToComb(nextProjectComb?.id ?? null);
        }}
        onSelectWorkspace={(combId) => {
          const comb = combs.find((item) => item.id === combId);
          if (!comb) return;
          navigateToComb(comb.id);
        }}
        canGoBackWorktree={canGoWorktreeBack}
        canGoForwardWorktree={canGoWorktreeForward}
        onWorktreeHistoryBack={() => goWorktreeBack()}
        onWorktreeHistoryForward={() => goWorktreeForward()}
        onSelectPane={(paneId) => {
          setActivePaneId(paneId);
          markPaneAttentionAsRead(paneId);
        }}
        onLaunchCommand={launchManagedCommand}
      />
    </div>
  );
}
