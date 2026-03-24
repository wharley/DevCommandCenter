"use client";

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  Bot,
  ChartNoAxesColumn,
  ChevronDown,
  CopyPlus,
  FolderGit2,
  GitBranch,
  Loader2,
  Plus,
  Settings,
  Terminal,
  Trash2,
} from "lucide-react";
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
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { EmbeddedTerminal } from "@/components/embedded-terminal";
import { CombReviewPanel } from "@/components/review/comb-review-panel";
import { AddProjectDialog } from "@/components/dialogs/add-project-dialog";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import {
  useCombs,
  usePanes,
  useProjects,
  useProviders,
} from "@/hooks/use-data";
import SettingsPage from "@/src/pages/SettingsPage";
import DashboardPage from "@/src/pages/DashboardPage";
import type { Comb, CreateCombDTO, Pane, Project, Provider } from "@/lib/database/types";

import {
  useDashboardMetrics,
  type DashboardPeriodDays,
} from "@/hooks/use-dashboard-metrics";
import {
  canAccessDashboard,
  getDashboardAccessContext,
} from "@/lib/dashboard/access";
import { useTerminalProjectActivity } from "@/hooks/use-terminal-project-activity";
import { useTerminalAttentionToasts } from "@/hooks/use-terminal-attention-toasts";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import { shouldSuggestStoryRef, recordProductSignal } from "@/lib/product/signals";
import { toast } from "sonner";

const CLI_PROVIDER_TYPES = [
  "codex",
  "claude-code",
  "gemini",
  "cursor",
] as const;

function isCliProviderType(
  type: string,
): type is (typeof CLI_PROVIDER_TYPES)[number] {
  return CLI_PROVIDER_TYPES.includes(
    type as (typeof CLI_PROVIDER_TYPES)[number],
  );
}

function buildCliCommand(provider: Provider | null): string | undefined {
  if (!provider) return undefined;
  const t = provider.type;
  const cliPath = provider.cliPath?.trim();
  const usePath =
    cliPath && (cliPath.startsWith("/") || /^[A-Za-z]:\\/.test(cliPath));
  if (t === "codex") return usePath ? cliPath : "codex";
  if (t === "claude-code") return usePath ? cliPath : "claude";
  if (t === "gemini") return usePath ? cliPath : "gemini";
  if (t === "cursor") return usePath ? cliPath : "cursor-agent";
  return undefined;
}

function getCombStatusLabel(comb: Comb): string {
  switch (comb.status) {
    case "active":
      return "Ativo";
    case "ready_for_review":
      return "Revisão";
    case "applied":
      return "Aplicado";
    case "discarded":
      return "Descartado";
    case "archived":
      return "Arquivado";
    case "error":
      return "Erro";
    default:
      return comb.status;
  }
}

function getCombStatusVariant(status: Comb["status"]) {
  switch (status) {
    case "active":
      return "default" as const;
    case "ready_for_review":
      return "secondary" as const;
    case "applied":
      return "outline" as const;
    case "discarded":
      return "outline" as const;
    case "error":
      return "destructive" as const;
    default:
      return "outline" as const;
  }
}

/** 1 col, 2–4 panes → 2 cols, 5+ → 3 cols (grid auto-wraps rows). */
function getPaneGridColumnCount(paneCount: number): number {
  if (paneCount <= 1) return 1;
  if (paneCount <= 4) return 2;
  return 3;
}

/** Lista branches locais via `desktopAPI.git` e permite escolher a branch base (com busca). */
function BranchBasePicker({
  projectPath,
  value,
  onValueChange,
  dialogOpen,
}: {
  projectPath?: string;
  value: string;
  onValueChange: (v: string) => void;
  dialogOpen: boolean;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [branches, setBranches] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [manualFallback, setManualFallback] = useState(false);

  const displayBranches = useMemo(() => {
    const v = value.trim();
    if (!v) return branches;
    if (branches.includes(v)) return branches;
    return [v, ...branches];
  }, [branches, value]);

  useEffect(() => {
    if (!dialogOpen) {
      setManualFallback(false);
      return;
    }
    if (!projectPath?.trim()) {
      setBranches([]);
      setManualFallback(true);
      return;
    }
    const git = window.desktopAPI?.git;
    if (!git?.getLocalBranches || !git?.getCurrentBranch) {
      setBranches([]);
      setManualFallback(true);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setManualFallback(false);
    Promise.all([
      git.getLocalBranches(projectPath),
      git.getCurrentBranch(projectPath),
    ])
      .then(([list, current]) => {
        if (cancelled) return;
        const arr = list ?? [];
        setBranches(arr);
        if (arr.length === 0) setManualFallback(true);
        if (!value.trim() && current?.trim()) {
          onValueChange(current.trim());
        }
      })
      .catch(() => {
        if (!cancelled) {
          setBranches([]);
          setManualFallback(true);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // value/onValueChange omitidos de propósito: só aplicar branch atual ao abrir o diálogo.
  }, [dialogOpen, projectPath]);

  if (loading) {
    return (
      <div className="flex h-10 items-center rounded-md border border-input bg-background px-3 text-sm text-muted-foreground">
        <Loader2 className="mr-2 h-4 w-4 shrink-0 animate-spin" />
        Carregando branches…
      </div>
    );
  }

  if (manualFallback) {
    return (
      <Input
        placeholder="main"
        value={value}
        onChange={(e) => onValueChange(e.target.value)}
      />
    );
  }

  return (
    <Popover open={menuOpen} onOpenChange={setMenuOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          role="combobox"
          aria-expanded={menuOpen}
          className="w-full justify-between font-normal"
        >
          <span className="truncate">
            {value.trim() ? (
              value
            ) : (
              <span className="text-muted-foreground">Selecione a branch base</span>
            )}
          </span>
          <ChevronDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[var(--radix-popover-trigger-width)] p-0" align="start">
        <Command>
          <CommandInput placeholder="Buscar branch…" className="h-9" />
          <CommandList>
            <CommandEmpty>Nenhuma branch encontrada.</CommandEmpty>
            <CommandGroup>
              {displayBranches.map((b) => (
                <CommandItem
                  key={b}
                  value={b}
                  onSelect={() => {
                    onValueChange(b);
                    setMenuOpen(false);
                  }}
                >
                  <GitBranch className="mr-2 h-4 w-4 shrink-0 opacity-70" />
                  <span className="truncate">{b}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

// ==========================================
// New Comb Dialog
// ==========================================
function NewCombDialog({
  open,
  onOpenChange,
  projectId,
  projectPath,
  onCreate,
  createComb,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  projectId: string;
  projectPath?: string;
  onCreate: (comb: Comb) => void;
  createComb: (data: CreateCombDTO) => Promise<Comb>;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [baseBranch, setBaseBranch] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [suggestStoryRef, setSuggestStoryRef] = useState(false);

  useEffect(() => {
    if (!open) return;
    setSuggestStoryRef(shouldSuggestStoryRef());
    setBaseBranch("");
  }, [open]);

  const handleCreate = async () => {
    if (!name.trim()) {
      toast.error("Nome do Workspace é obrigatório");
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
      toast.success("Workspace criado");
      onCreate(comb);
      onOpenChange(false);
      setName("");
      setDescription("");
    } catch {
      toast.error("Falha ao criar Workspace");
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Novo Workspace</DialogTitle>
          <DialogDescription>
            Cria um ambiente isolado (worktree) para uma história de negócio.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <div className="rounded-md border border-border bg-muted/30 p-2">
            <p className="text-xs text-muted-foreground">
              Padrão recomendado:{" "}
              <span className="font-medium">1 Workspace por história</span>. Se
              precisar mexer em mais de um repo, adicione repositórios extras
              na aba Review.
            </p>
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Nome</label>
            <Input
              placeholder="ex.: feature-auth"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Descrição (opcional)</label>
            <Textarea
              placeholder="O que este Workspace faz..."
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              className="field-sizing-fixed max-h-40 overflow-y-auto"
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Branch base</label>
            <BranchBasePicker
              projectPath={projectPath}
              value={baseBranch}
              onValueChange={setBaseBranch}
              dialogOpen={open}
            />
          </div>
          {suggestStoryRef ? (
            <p className="text-xs text-muted-foreground">
              Você está em fluxo cross-repo recorrente. Enquanto não existe
              campo dedicado de história compartilhada, use um prefixo no nome
              do Workspace (ex.:{" "}
              <span className="font-mono">US-1234 - Campo X</span>).
            </p>
          ) : null}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancelar
          </Button>
          <Button onClick={handleCreate} disabled={isCreating}>
            {isCreating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Criar Workspace
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function MirrorMissionDialog({
  open,
  onOpenChange,
  sourceComb,
  sourceProjectId,
  projects,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  sourceComb: Comb;
  sourceProjectId: string;
  projects: Project[];
  onCreated: (targetProjectId: string, combId: string) => void;
}) {
  const [targetProjectId, setTargetProjectId] = useState("");
  const [mirrorName, setMirrorName] = useState("");
  const [mirrorDescription, setMirrorDescription] = useState("");
  const [baseBranch, setBaseBranch] = useState("");
  const [isCreating, setIsCreating] = useState(false);

  const targetProjectOptions = useMemo(
    () => projects.filter((p) => p.id !== sourceProjectId),
    [projects, sourceProjectId],
  );

  const targetProject = useMemo(
    () => projects.find((p) => p.id === targetProjectId) ?? null,
    [projects, targetProjectId],
  );

  useEffect(() => {
    if (!open) return;
    setMirrorName(sourceComb.name);
    setMirrorDescription(sourceComb.description ?? "");
    setBaseBranch("");
    const firstTarget = targetProjectOptions[0];
    setTargetProjectId(firstTarget?.id ?? "");
  }, [open, sourceComb, targetProjectOptions]);

  const handleCreate = async () => {
    if (!targetProjectId || !mirrorName.trim()) {
      toast.error("Preencha projeto e nome do Workspace espelho.");
      return;
    }
    if (!window.db?.combs?.create) {
      toast.error("Criação de Workspace indisponível.");
      return;
    }
    setIsCreating(true);
    try {
      const created = (await window.db.combs.create({
        projectId: targetProjectId,
        name: mirrorName.trim(),
        description: mirrorDescription.trim() || undefined,
        baseBranch: baseBranch.trim() || "main",
      })) as { id?: string };
      if (!created?.id) throw new Error("missing comb id");
      recordProductSignal("mirror_mission_created");
      toast.success("Workspace espelho criado");
      onCreated(targetProjectId, created.id);
      onOpenChange(false);
    } catch {
      toast.error("Falha ao criar Workspace espelho");
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Criar Workspace espelho</DialogTitle>
          <DialogDescription>
            Copia nome e descrição do Workspace atual para outro projeto, sem
            reescrever o contexto da história.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 py-2">
          <div className="space-y-2">
            <label className="text-sm font-medium">Projeto de destino</label>
            {targetProjectOptions.length > 0 ? (
              <Select
                value={targetProjectId}
                onValueChange={(id) => {
                  setTargetProjectId(id);
                  setBaseBranch("");
                }}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Selecione o projeto" />
                </SelectTrigger>
                <SelectContent>
                  {targetProjectOptions.map((p) => (
                    <SelectItem key={p.id} value={p.id}>
                      {p.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <p className="text-xs text-muted-foreground">
                Não há outro projeto disponível para espelhar.
              </p>
            )}
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Nome</label>
            <Input
              value={mirrorName}
              onChange={(e) => setMirrorName(e.target.value)}
              placeholder="ex.: US-1234 - Campo X"
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Descrição</label>
            <Textarea
              value={mirrorDescription}
              onChange={(e) => setMirrorDescription(e.target.value)}
              rows={3}
              className="field-sizing-fixed max-h-40 overflow-y-auto"
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Branch base</label>
            <BranchBasePicker
              key={targetProject?.path ?? targetProjectId}
              projectPath={targetProject?.path}
              value={baseBranch}
              onValueChange={setBaseBranch}
              dialogOpen={open}
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancelar
          </Button>
          <Button
            onClick={handleCreate}
            disabled={
              isCreating || !targetProjectId || targetProjectOptions.length === 0
            }
          >
            {isCreating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Criar espelho
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ==========================================
// New Agent Pane Dialog
// ==========================================
function NewAgentPaneDialog({
  open,
  onOpenChange,
  combId,
  providers,
  onCreate,
  ensureCombWorktree,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  combId: string;
  providers: Provider[];
  onCreate: (pane: Pane) => void;
  ensureCombWorktree: () => Promise<boolean>;
}) {
  const [providerId, setProviderId] = useState("");
  const [prompt, setPrompt] = useState("");
  const [title, setTitle] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const { create } = usePanes(combId);

  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProviderType(p.type)),
    [providers],
  );

  const handleCreate = async () => {
    setIsCreating(true);
    try {
      const wtOk = await ensureCombWorktree();
      if (!wtOk) return;
      const pane = await create({
        combId,
        type: "agent",
        providerId: providerId || undefined,
        title: title.trim() || undefined,
        initialPrompt: prompt.trim() || undefined,
      });
      toast.success("Agent pane criado");
      onCreate(pane);
      onOpenChange(false);
      setProviderId("");
      setPrompt("");
      setTitle("");
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
          <DialogDescription>
            Abre um agente CLI neste Workspace.
          </DialogDescription>
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
          <div className="space-y-2">
            <label className="text-sm font-medium">Titulo (opcional)</label>
            <Input
              placeholder="ex.: Implementar OAuth"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">
              Prompt inicial (opcional)
            </label>
            <Textarea
              placeholder="Instrução para o agente..."
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={3}
              className="field-sizing-fixed max-h-48 overflow-y-auto"
            />
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

// ==========================================
// Pane Grid Item
// ==========================================
function PaneGridItem({
  pane,
  comb,
  provider,
  onRemove,
}: {
  pane: Pane;
  comb: Comb;
  provider: Provider | null;
  onRemove: () => void;
}) {
  const cwd = comb.worktreePath ?? "";
  const command = pane.type === "agent" ? buildCliCommand(provider) : undefined;
  const args =
    pane.type === "agent" && pane.initialPrompt ? [pane.initialPrompt] : [];

  const label =
    pane.type === "agent"
      ? (pane.title ?? provider?.name ?? "Agent")
      : (pane.title ?? "Terminal");

  if (!cwd) {
    return (
      <div className="flex h-full min-h-0 min-w-0 flex-col items-center justify-center overflow-hidden rounded-lg border border-border bg-card p-4">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        <p className="mt-2 text-sm text-muted-foreground">
          Preparando worktree...
        </p>
      </div>
    );
  }

  return (
    <div
      data-pane-id={pane.id}
      className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden"
    >
      <div className="flex items-center justify-between border-b border-border bg-card px-3 py-1.5">
        <div className="flex items-center gap-2">
          {pane.type === "agent" ? (
            <Bot className="h-3.5 w-3.5 text-primary" />
          ) : (
            <Terminal className="h-3.5 w-3.5 text-muted-foreground" />
          )}
          <span className="text-xs font-medium uppercase tracking-wider">
            {pane.type === "agent" ? "AGENT" : "TERM"}
          </span>
          <span className="text-xs text-muted-foreground truncate max-w-[120px]">
            {label}
          </span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6"
          onClick={onRemove}
        >
          <Trash2 className="h-3 w-3" />
        </Button>
      </div>
      <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
        <EmbeddedTerminal
          cwd={cwd}
          command={command}
          args={args}
          paneId={pane.id}
          title={label}
        />
      </div>
    </div>
  );
}

// ==========================================
// Main Page — Beehive-style full-screen layout
// ==========================================
export default function HiveWorkspacePage() {
  const { projects, isLoading: projectsLoading } = useProjects();
  const { providers } = useProviders();
  const { confirmDialog } = useConfirmDialog();

  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(
    null,
  );
  const [activeCombId, setActiveCombId] = useState<string | null>(null);
  const [activeMainTab, setActiveMainTab] = useState<"panes" | "review">(
    "panes",
  );
  const [newCombOpen, setNewCombOpen] = useState(false);
  const [newAgentOpen, setNewAgentOpen] = useState(false);
  const [addProjectOpen, setAddProjectOpen] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showDashboard, setShowDashboard] = useState(false);
  const [mirrorMissionOpen, setMirrorMissionOpen] = useState(false);
  const [dashboardPeriodDays, setDashboardPeriodDays] =
    useState<DashboardPeriodDays>(7);
  const [projectPickerOpen, setProjectPickerOpen] = useState(false);

  const safeDateMs = (value: unknown): number => {
    if (value instanceof Date) return value.getTime();
    if (typeof value === "string") {
      const ms = Date.parse(value);
      return Number.isNaN(ms) ? 0 : ms;
    }
    return 0;
  };

  const sortedProjects = useMemo(
    () =>
      [...projects].sort((a, b) => {
        const da = safeDateMs(a.lastOpenedAt) || safeDateMs(a.createdAt);
        const db = safeDateMs(b.lastOpenedAt) || safeDateMs(b.createdAt);
        return db - da;
      }),
    [projects],
  );

  const selectedProject = useMemo(
    () =>
      selectedProjectId
        ? (projects.find((p) => p.id === selectedProjectId) ?? null)
        : null,
    [selectedProjectId, projects],
  );

  const {
    combs,
    isLoading: combsLoading,
    refresh: refreshCombs,
    create: createComb,
    update: updateComb,
  } = useCombs(selectedProjectId ?? undefined);

  const combsRef = useRef(combs);
  combsRef.current = combs;

  const activeComb = useMemo(
    () =>
      activeCombId ? (combs.find((c) => c.id === activeCombId) ?? null) : null,
    [activeCombId, combs],
  );

  /** Ref evita que `combs` no deps recrie o callback a cada refresh e dispare o efeito de worktree em loop. */
  const ensureActiveCombWorktree = useCallback(async (): Promise<boolean> => {
    if (!activeCombId) return false;
    const comb = combsRef.current.find((c) => c.id === activeCombId);
    if (!comb) return false;
    if (comb.worktreePath) return true;
    const api = window.desktopAPI?.comb?.ensureWorktree;
    if (!api) {
      toast.error("Worktree indisponível neste ambiente.");
      return false;
    }
    try {
      const result = await api(activeCombId);
      if (result.success) {
        await refreshCombs();
        return true;
      }
      if (result.error) toast.error(`Worktree: ${result.error}`);
      return false;
    } catch (e: unknown) {
      const msg =
        e instanceof Error
          ? e.message
          : typeof e === "string"
            ? e
            : "Falha ao preparar worktree";
      toast.error(msg);
      return false;
    }
  }, [activeCombId, refreshCombs]);

  const {
    panes,
    refresh: refreshPanes,
    create: createPane,
    remove: removePane,
  } = usePanes(activeCombId ?? undefined);
  const { activity: terminalActivity, refresh: refreshTerminalActivity } =
    useTerminalProjectActivity(selectedProjectId);

  useTerminalAttentionToasts({
    onNavigateToPane: ({ projectId, combId, paneId }) => {
      setSelectedProjectId(projectId);
      setActiveCombId(combId);
      setActiveMainTab("panes");
      requestAnimationFrame(() => {
        document
          .querySelector(`[data-pane-id="${paneId}"]`)
          ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
      });
    },
  });

  const dashboardAccess = useMemo(() => {
    const ctx = getDashboardAccessContext();
    return canAccessDashboard(ctx);
  }, []);

  const dashboardMetrics = useDashboardMetrics(
    combs,
    dashboardPeriodDays,
  );

  const providerById = useMemo(() => {
    const map = new Map<string, Provider>();
    for (const p of providers) map.set(p.id, p);
    return map;
  }, [providers]);

  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProviderType(p.type)),
    [providers],
  );

  // Restore selected project from localStorage
  useEffect(() => {
    if (selectedProjectId && projects.some((p) => p.id === selectedProjectId))
      return;
    const stored = localStorage.getItem("dcc:hive:selectedProject");
    if (stored && projects.some((p) => p.id === stored)) {
      setSelectedProjectId(stored);
      return;
    }
    if (projects.length > 0) {
      const sorted = [...projects].sort((a, b) => {
        const da = safeDateMs(a.lastOpenedAt) || safeDateMs(a.createdAt);
        const db = safeDateMs(b.lastOpenedAt) || safeDateMs(b.createdAt);
        return db - da;
      });
      setSelectedProjectId(sorted[0].id);
    }
  }, [projects, selectedProjectId]);

  useEffect(() => {
    if (selectedProjectId) {
      localStorage.setItem("dcc:hive:selectedProject", selectedProjectId);
    }
  }, [selectedProjectId]);

  // Auto-select comb or restore from localStorage
  useEffect(() => {
    if (!selectedProjectId) {
      setActiveCombId(null);
      return;
    }
    if (activeCombId && combs.some((c) => c.id === activeCombId)) return;
    const stored = localStorage.getItem(
      `dcc:hive:${selectedProjectId}:activeComb`,
    );
    if (stored && combs.some((c) => c.id === stored)) {
      setActiveCombId(stored);
      return;
    }
    setActiveCombId(combs.length > 0 ? combs[0].id : null);
  }, [combs, activeCombId, selectedProjectId]);

  useEffect(() => {
    if (activeCombId && selectedProjectId) {
      localStorage.setItem(
        `dcc:hive:${selectedProjectId}:activeComb`,
        activeCombId,
      );
    }
  }, [activeCombId, selectedProjectId]);

  useEffect(() => {
    const goPanes = () => setActiveMainTab("panes");
    window.addEventListener("dcc:hive:goto-panes", goPanes);
    return () => window.removeEventListener("dcc:hive:goto-panes", goPanes);
  }, []);

  // Ensure worktree quando a Missão fica ativa (deps só id/path — evita loop com cada refresh de `combs`)
  const activeCombIdForWt = activeComb?.id ?? null;
  const activeCombWtPath = activeComb?.worktreePath ?? null;
  useEffect(() => {
    if (!activeCombIdForWt || activeCombWtPath) return;
    const ensureWorktree = window.desktopAPI?.comb?.ensureWorktree;
    if (!ensureWorktree) return;
    let cancelled = false;
    void (async () => {
      try {
        const result = await ensureWorktree(activeCombIdForWt);
        if (cancelled) return;
        if (result.success) await refreshCombs();
        else if (result.error) toast.error(`Worktree: ${result.error}`);
      } catch (e: unknown) {
        if (!cancelled) {
          const msg =
            e instanceof Error
              ? e.message
              : typeof e === "string"
                ? e
                : "Falha ao preparar worktree";
          toast.error(msg);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeCombIdForWt, activeCombWtPath, refreshCombs]);

  const handleProjectChange = (projectId: string) => {
    setSelectedProjectId(projectId);
    setActiveCombId(null);
    setActiveMainTab("panes");
    setShowSettings(false);
    setShowDashboard(false);
  };

  const handleCombCreated = (comb: Comb) => {
    setActiveCombId(comb.id);
    setShowSettings(false);
    setShowDashboard(false);
    refreshCombs();
  };

  const handleMirrorMissionCreated = (targetProjectId: string, combId: string) => {
    localStorage.setItem(`dcc:hive:${targetProjectId}:activeComb`, combId);
    setSelectedProjectId(targetProjectId);
    setActiveCombId(combId);
    setActiveMainTab("panes");
    setShowSettings(false);
    setShowDashboard(false);
  };

  const handleAddTerminal = async () => {
    if (!activeCombId) return;
    try {
      const ok = await ensureActiveCombWorktree();
      if (!ok) return;
      await createPane({ combId: activeCombId, type: "term" });
      refreshPanes();
      void refreshTerminalActivity();
    } catch {
      toast.error("Falha ao criar terminal");
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
    await removePane(paneId);
    refreshPanes();
    void refreshTerminalActivity();
  };

  const handleRemoveComb = async (comb_id: string) => {
    const confirmed = await confirmDialog({
      title: "Remover Workspace?",
      description: "O worktree e todos os panes serão removidos.",
      confirmLabel: "Remover",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    if (window.desktopAPI?.comb?.discard) {
      const result = await window.desktopAPI.comb.discard(comb_id);
      if (!result.success && result.error) toast.error(result.error);
    }
    if (window.db?.combs) await window.db.combs.delete(comb_id);
    if (activeCombId === comb_id) setActiveCombId(null);
    refreshCombs();
    toast.success("Workspace removido");
  };

  if (projectsLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-background">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="flex h-screen overflow-hidden bg-background">
      {/* Titlebar drag area */}
      <div className="titlebar-drag-region fixed top-0 left-0 right-0 h-8 z-50" />

      {/* ====== SIDEBAR ====== */}
      <aside className="flex w-64 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
        {/* Logo + Hive selector */}
        <div className="border-b border-sidebar-border px-4 pt-10 pb-3">
          <div className="mb-3 flex items-center gap-2">
            <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-sidebar-primary">
              <Terminal className="h-4 w-4 text-sidebar-primary-foreground" />
            </div>
            <span className="text-sm font-semibold tracking-tight">
              Dev Command
            </span>
          </div>

          {/* Hive (Project) Selector — searchable */}
          <Popover open={projectPickerOpen} onOpenChange={setProjectPickerOpen}>
            <PopoverTrigger asChild>
              <button
                type="button"
                className="titlebar-no-drag flex w-full items-center justify-between rounded-md border border-sidebar-border bg-sidebar-accent/30 px-3 py-2 text-sm transition-colors hover:bg-sidebar-accent/50"
              >
                <div className="flex items-center gap-2 min-w-0">
                  <FolderGit2 className="h-4 w-4 shrink-0 text-sidebar-foreground/70" />
                  <span className="truncate font-medium">
                    {selectedProject?.name ?? "Selecionar Hive"}
                  </span>
                </div>
                <ChevronDown className="h-3.5 w-3.5 shrink-0 text-sidebar-foreground/50" />
              </button>
            </PopoverTrigger>
            <PopoverContent className="w-56 p-0" align="start">
              <Command className="rounded-lg border-0 bg-popover">
                <CommandInput placeholder="Buscar projeto…" className="h-9" />
                <CommandList>
                  <CommandEmpty>Nenhum projeto encontrado.</CommandEmpty>
                  <CommandGroup heading="Projetos">
                    {sortedProjects.map((p) => (
                      <CommandItem
                        key={p.id}
                        value={`${p.name} ${p.path}`}
                        onSelect={() => {
                          handleProjectChange(p.id);
                          setProjectPickerOpen(false);
                        }}
                        className={
                          p.id === selectedProjectId ? "bg-accent" : ""
                        }
                      >
                        <FolderGit2 className="mr-2 h-4 w-4" />
                        <span className="truncate">{p.name}</span>
                      </CommandItem>
                    ))}
                  </CommandGroup>
                  <CommandSeparator />
                  <CommandGroup>
                    <CommandItem
                      value="adicionar-projeto"
                      onSelect={() => {
                        setAddProjectOpen(true);
                        setProjectPickerOpen(false);
                      }}
                    >
                      <Plus className="mr-2 h-4 w-4" />
                      Adicionar projeto
                    </CommandItem>
                  </CommandGroup>
                </CommandList>
              </Command>
            </PopoverContent>
          </Popover>
        </div>

        {/* Missões section header */}
        <div className="flex items-center justify-between px-4 py-2 gap-2">
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="text-xs font-medium uppercase tracking-wider text-sidebar-foreground/50">
              Workspaces
            </span>
            {terminalActivity.totalRunningPanes > 0 && (
              <span className="text-[10px] leading-tight text-emerald-600/90 dark:text-emerald-400/90">
                {terminalActivity.totalRunningPanes}{" "}
                {terminalActivity.totalRunningPanes === 1
                  ? "sessão ativa"
                  : "sessões ativas"}{" "}
                neste projeto
              </span>
            )}
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="titlebar-no-drag h-6 w-6 text-sidebar-foreground/50 hover:bg-sidebar-accent hover:text-sidebar-foreground"
            onClick={() => setNewCombOpen(true)}
            disabled={!selectedProjectId}
          >
            <Plus className="h-3.5 w-3.5" />
          </Button>
        </div>

        {/* Combs list */}
        <div className="flex-1 min-h-0 overflow-y-auto px-2">
          {!selectedProjectId ? (
            <p className="px-3 py-4 text-center text-xs text-sidebar-foreground/40">
              Selecione um projeto acima
            </p>
          ) : combsLoading ? (
            <div className="flex justify-center py-8">
              <Loader2 className="h-5 w-5 animate-spin text-sidebar-foreground/40" />
            </div>
          ) : combs.length === 0 ? (
            <div className="px-3 py-4 text-center">
              <p className="text-xs text-sidebar-foreground/40">
                Nenhum Workspace ainda
              </p>
              <Button
                variant="ghost"
                size="sm"
                className="titlebar-no-drag mt-2 text-xs"
                onClick={() => setNewCombOpen(true)}
              >
                <Plus className="mr-1 h-3 w-3" />
                Criar primeiro Workspace
              </Button>
            </div>
          ) : (
            <div className="flex flex-col gap-1 py-1">
              {combs.map((comb) => {
                const isActive = comb.id === activeCombId;
                const runningPanes =
                  terminalActivity.runningPanesByCombId[comb.id] ?? 0;
                return (
                  <div
                    key={comb.id}
                    role="button"
                    tabIndex={0}
                    onClick={() => {
                      setActiveCombId(comb.id);
                      setShowSettings(false);
                      setShowDashboard(false);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        setActiveCombId(comb.id);
                        setShowSettings(false);
                        setShowDashboard(false);
                      }
                    }}
                    className={`titlebar-no-drag group flex w-full cursor-pointer gap-1.5 rounded-lg px-3 py-2 text-left transition-colors ${
                      isActive
                        ? "bg-sidebar-accent text-sidebar-accent-foreground"
                        : "text-sidebar-foreground/70 hover:bg-sidebar-accent/50 hover:text-sidebar-foreground"
                    }`}
                  >
                    <div className="min-w-0 flex-1 flex flex-col">
                      <span className="flex items-center gap-1.5 truncate text-sm font-medium">
                        {comb.name}
                        {runningPanes > 0 && (
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <span className="inline-flex shrink-0">
                                <Activity
                                  className="h-3.5 w-3.5 text-emerald-500"
                                  aria-hidden
                                />
                              </span>
                            </TooltipTrigger>
                            <TooltipContent side="right">
                              {runningPanes === 1
                                ? "1 pane com sessão ativa (terminal ou agent)"
                                : `${runningPanes} panes com sessão ativa (terminais ou agents)`}
                            </TooltipContent>
                          </Tooltip>
                        )}
                      </span>
                      <div className="mt-0.5 flex items-center gap-1.5">
                        <GitBranch className="h-3 w-3 shrink-0 text-sidebar-foreground/40" />
                        <span className="truncate text-[11px] text-sidebar-foreground/50">
                          {comb.branch ?? comb.baseBranch}
                        </span>
                      </div>
                      <div className="mt-1 flex items-center gap-2">
                        <Badge
                          variant={getCombStatusVariant(comb.status)}
                          className="text-[10px] px-1.5 py-0"
                        >
                          {getCombStatusLabel(comb)}
                        </Badge>
                      </div>
                    </div>
                    <div className="shrink-0 self-start pt-0.5">
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        className="h-5 w-5 min-h-5 min-w-5 p-0 opacity-0 group-hover:opacity-100 text-sidebar-foreground/50 hover:text-destructive"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleRemoveComb(comb.id);
                        }}
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Sidebar footer */}
        <div className="border-t border-sidebar-border p-2 space-y-1">
          <Button
            variant="ghost"
            size="sm"
            className="titlebar-no-drag w-full justify-start gap-3 text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground"
            onClick={() => {
              if (!dashboardAccess.enabled) return;
              setShowDashboard(true);
              setShowSettings(false);
              setActiveCombId(null);
            }}
            disabled={!dashboardAccess.enabled}
          >
            <ChartNoAxesColumn className="h-4 w-4" />
            Dashboard
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="titlebar-no-drag w-full justify-start gap-3 text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground"
            onClick={() => {
              setShowSettings(true);
              setShowDashboard(false);
              setActiveCombId(null);
            }}
          >
            <Settings className="h-4 w-4" />
            Configurações
          </Button>
        </div>
      </aside>

      {/* ====== MAIN CONTENT ====== */}
      <main className="flex min-h-0 flex-1 flex-col">
        {showDashboard ? (
          <div className="flex-1 overflow-auto mt-8">
            <DashboardPage
              selectedProjectName={selectedProject?.name}
              periodDays={dashboardPeriodDays}
              onPeriodChange={setDashboardPeriodDays}
              metrics={dashboardMetrics}
            />
          </div>
        ) : showSettings ? (
          <div className="flex-1 overflow-auto mt-8">
            <SettingsPage />
          </div>
        ) : activeComb ? (
          <>
            {/* Toolbar */}
            <div className="flex items-center justify-between border-b border-border px-4 py-2 mt-8">
              <div className="flex items-center gap-3">
                <h3 className="text-sm font-semibold">{activeComb.name}</h3>
                <Badge variant="outline" className="gap-1">
                  <GitBranch className="h-3 w-3" />
                  {activeComb.branch ?? activeComb.baseBranch}
                </Badge>
                {activeComb.description && (
                  <span className="text-xs text-muted-foreground truncate max-w-xs">
                    {activeComb.description}
                  </span>
                )}
              </div>
              <div className="flex items-center gap-2">
                <div className="flex rounded-md border border-border">
                  <button
                    onClick={() => setActiveMainTab("panes")}
                    className={`px-3 py-1 text-xs font-medium transition-colors ${
                      activeMainTab === "panes"
                        ? "bg-primary text-primary-foreground"
                        : "hover:bg-muted"
                    } rounded-l-md`}
                  >
                    Panes
                  </button>
                  <button
                    onClick={() => setActiveMainTab("review")}
                    className={`px-3 py-1 text-xs font-medium transition-colors ${
                      activeMainTab === "review"
                        ? "bg-primary text-primary-foreground"
                        : "hover:bg-muted"
                    } rounded-r-md`}
                  >
                    Review
                  </button>
                </div>
                {activeMainTab === "panes" && (
                  <>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setMirrorMissionOpen(true)}
                      disabled={sortedProjects.length < 2}
                    >
                      <CopyPlus className="mr-1 h-3 w-3" />
                      Espelhar
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handleAddTerminal}
                    >
                      <Terminal className="mr-1 h-3 w-3" />
                      Terminal
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setNewAgentOpen(true)}
                      disabled={cliProviders.length === 0}
                    >
                      <Bot className="mr-1 h-3 w-3" />
                      Agent
                    </Button>
                  </>
                )}
              </div>
            </div>

            {/* Panes + Review: ambos montados para não desmontar EmbeddedTerminal ao trocar aba */}
            <div className="min-h-0 flex-1 overflow-hidden">
              <div
                className={
                  activeMainTab === "panes"
                    ? "flex h-full min-h-0 flex-col overflow-hidden"
                    : "hidden"
                }
                aria-hidden={activeMainTab !== "panes"}
              >
                {panes.length === 0 ? (
                  <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
                    <Terminal className="h-12 w-12 text-muted-foreground/30" />
                    <p className="text-sm text-muted-foreground">
                      Nenhum pane aberto neste Workspace
                    </p>
                    <div className="flex gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={handleAddTerminal}
                      >
                        <Terminal className="mr-1 h-3 w-3" />
                        Abrir Terminal
                      </Button>
                      {cliProviders.length > 0 && (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setNewAgentOpen(true)}
                        >
                          <Bot className="mr-1 h-3 w-3" />
                          Abrir Agent
                        </Button>
                      )}
                    </div>
                  </div>
                ) : (
                  <div
                    className="grid h-full min-h-0 min-w-0 auto-rows-fr gap-1 overflow-hidden p-1"
                    style={{
                      gridTemplateColumns: `repeat(${getPaneGridColumnCount(panes.length)}, minmax(0, 1fr))`,
                    }}
                  >
                    {panes.map((pane) => (
                      <PaneGridItem
                        key={pane.id}
                        pane={pane}
                        comb={activeComb}
                        provider={
                          pane.providerId
                            ? (providerById.get(pane.providerId) ?? null)
                            : null
                        }
                        onRemove={() => handleRemovePane(pane.id)}
                      />
                    ))}
                  </div>
                )}
              </div>
              <div
                className={
                  activeMainTab === "review"
                    ? "flex h-full min-h-0 flex-col overflow-hidden"
                    : "hidden"
                }
                aria-hidden={activeMainTab !== "review"}
              >
                <CombReviewPanel
                  comb={activeComb}
                  mainProjectPath={selectedProject?.path}
                  projects={sortedProjects}
                  updateComb={updateComb}
                  onAction={() => refreshCombs()}
                />
              </div>
            </div>
          </>
        ) : selectedProjectId ? (
          <div className="flex h-full flex-col items-center justify-center gap-4 p-8 mt-8">
            <FolderGit2 className="h-16 w-16 text-muted-foreground/20" />
            <div className="text-center">
              <h3 className="text-lg font-medium">Comece criando um Workspace</h3>
              <p className="mt-1 text-sm text-muted-foreground">
                Cada Workspace cria uma worktree isolada onde seus agents e
                terminais compartilham o mesmo espaço.
              </p>
            </div>
            <Button onClick={() => setNewCombOpen(true)}>
              <Plus className="mr-2 h-4 w-4" />
              Novo Workspace
            </Button>
          </div>
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-4 p-8 mt-8">
            <FolderGit2 className="h-16 w-16 text-muted-foreground/20" />
            <div className="text-center">
              <h3 className="text-lg font-medium">
                Selecione ou adicione um Hive
              </h3>
              <p className="mt-1 text-sm text-muted-foreground">
                Use o seletor no topo da sidebar para escolher um projeto.
              </p>
            </div>
            <Button onClick={() => setAddProjectOpen(true)}>
              <Plus className="mr-2 h-4 w-4" />
              Adicionar projeto
            </Button>
          </div>
        )}
      </main>

      {/* Dialogs */}
      {selectedProjectId && (
        <NewCombDialog
          open={newCombOpen}
          onOpenChange={setNewCombOpen}
          projectId={selectedProjectId}
          projectPath={selectedProject?.path}
          onCreate={handleCombCreated}
          createComb={createComb}
        />
      )}

      {activeCombId && (
        <NewAgentPaneDialog
          open={newAgentOpen}
          onOpenChange={setNewAgentOpen}
          combId={activeCombId}
          providers={providers}
          ensureCombWorktree={ensureActiveCombWorktree}
          onCreate={() => {
            refreshPanes();
            void refreshTerminalActivity();
          }}
        />
      )}

      {activeComb && selectedProjectId && (
        <MirrorMissionDialog
          open={mirrorMissionOpen}
          onOpenChange={setMirrorMissionOpen}
          sourceComb={activeComb}
          sourceProjectId={selectedProjectId}
          projects={sortedProjects}
          onCreated={handleMirrorMissionCreated}
        />
      )}

      <AddProjectDialog
        open={addProjectOpen}
        onOpenChange={setAddProjectOpen}
      />
    </div>
  );
}
