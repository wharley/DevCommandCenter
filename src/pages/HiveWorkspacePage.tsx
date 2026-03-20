"use client";

import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  Bot,
  ChevronDown,
  FileCode,
  FolderGit2,
  GitBranch,
  GitMerge,
  Loader2,
  Plus,
  Settings,
  Terminal,
  Trash2,
  Upload,
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
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { EmbeddedTerminal } from "@/components/embedded-terminal";
import { DiffCodeBlock } from "@/components/diff-code-block";
import { CommitDialog } from "@/components/dialogs/commit-dialog";
import { AddProjectDialog } from "@/components/dialogs/add-project-dialog";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import {
  useCombs,
  usePanes,
  useProjects,
  useProviders,
} from "@/hooks/use-data";
import SettingsPage from "@/src/pages/SettingsPage";
import type { Comb, Pane, Project, Provider } from "@/lib/database/types";
import type { GitStatus } from "@/types/electron";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
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

// ==========================================
// New Comb Dialog
// ==========================================
function NewCombDialog({
  open,
  onOpenChange,
  projectId,
  projectPath,
  onCreate,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  projectId: string;
  projectPath?: string;
  onCreate: (comb: Comb) => void;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [baseBranch, setBaseBranch] = useState("");
  const [localBranches, setLocalBranches] = useState<string[]>([]);
  const [isCreating, setIsCreating] = useState(false);
  const { create } = useCombs(projectId);

  useEffect(() => {
    if (!open || !projectPath) return;
    const git = window.electronAPI?.git;
    if (!git?.getLocalBranches || !git?.getCurrentBranch) return;
    let cancelled = false;
    Promise.all([
      git.getLocalBranches(projectPath),
      git.getCurrentBranch(projectPath),
    ]).then(([branches, current]) => {
      if (cancelled) return;
      setLocalBranches(branches ?? []);
      if (!baseBranch && current) setBaseBranch(current.trim());
    });
    return () => {
      cancelled = true;
    };
  }, [open, projectPath]);

  const handleCreate = async () => {
    if (!name.trim()) {
      toast.error("Nome da Missão é obrigatório");
      return;
    }
    setIsCreating(true);
    try {
      const comb = await create({
        projectId,
        name: name.trim(),
        description: description.trim() || undefined,
        baseBranch: baseBranch.trim() || "main",
      });
      toast.success("Missão criada");
      onCreate(comb);
      onOpenChange(false);
      setName("");
      setDescription("");
    } catch {
      toast.error("Falha ao criar Missão");
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Nova Missão</DialogTitle>
          <DialogDescription>
            Cria um workspace isolado (worktree) para uma feature ou tarefa.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2">
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
              placeholder="O que esta Missão faz..."
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              className="field-sizing-fixed max-h-40 overflow-y-auto"
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Branch base</label>
            {localBranches.length > 0 ? (
              <Select value={baseBranch} onValueChange={setBaseBranch}>
                <SelectTrigger>
                  <SelectValue placeholder="Selecione branch" />
                </SelectTrigger>
                <SelectContent>
                  {localBranches.map((b) => (
                    <SelectItem key={b} value={b}>
                      {b}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input
                placeholder="main"
                value={baseBranch}
                onChange={(e) => setBaseBranch(e.target.value)}
              />
            )}
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancelar
          </Button>
          <Button onClick={handleCreate} disabled={isCreating}>
            {isCreating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Criar Missão
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
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  combId: string;
  providers: Provider[];
  onCreate: (pane: Pane) => void;
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
            Abre um agente CLI nesta Missão.
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
    <div className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden">
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
// Comb Review Panel
// ==========================================
function CombReviewPanel({
  comb,
  mainProjectPath,
  onAction,
}: {
  comb: Comb;
  /** Repositório principal (checkout), não o worktree — usado para listar branches de destino do merge. */
  mainProjectPath?: string;
  onAction: () => void;
}) {
  const [diffs, setDiffs] = useState<{
    loading: boolean;
    error?: string;
    files: Array<{ path: string; status: string; diff: string }>;
    summary: {
      changedFiles: number;
      insertions: number;
      deletions: number;
    } | null;
  }>({ loading: false, files: [], summary: null });
  const [isApplying, setIsApplying] = useState(false);
  const [isMerging, setIsMerging] = useState(false);
  const [commitDialogOpen, setCommitDialogOpen] = useState(false);
  const [commitDialogStatus, setCommitDialogStatus] =
    useState<GitStatus | null>(null);
  const [mergeDialogOpen, setMergeDialogOpen] = useState(false);
  const [mergeTargetBranch, setMergeTargetBranch] = useState("");
  const [mergeBranches, setMergeBranches] = useState<string[]>([]);

  useEffect(() => {
    if (!comb.worktreePath || !window.electronAPI?.comb?.getDiffs) return;
    setDiffs((prev) => ({ ...prev, loading: true, error: undefined }));
    window.electronAPI.comb.getDiffs(comb.id).then((result) => {
      if (result.success) {
        setDiffs({
          loading: false,
          files: result.files,
          summary: result.summary,
        });
      } else {
        setDiffs({
          loading: false,
          error: result.error,
          files: [],
          summary: null,
        });
      }
    });
  }, [comb.id, comb.worktreePath]);

  useEffect(() => {
    if (!mergeDialogOpen || !mainProjectPath?.trim()) return;
    const git = window.electronAPI?.git;
    if (!git?.getLocalBranches || !git?.getCurrentBranch) return;
    let cancelled = false;
    Promise.all([
      git.getLocalBranches(mainProjectPath),
      git.getCurrentBranch(mainProjectPath),
    ]).then(([branches, current]) => {
      if (cancelled) return;
      const list = branches ?? [];
      setMergeBranches(list);
      const c = (current ?? "").trim();
      setMergeTargetBranch(c || list[0] || "main");
    });
    return () => {
      cancelled = true;
    };
  }, [mergeDialogOpen, mainProjectPath]);

  const handleMergeClick = () => {
    if (!mainProjectPath?.trim()) {
      toast.error("Caminho do repositório principal indisponível.");
      return;
    }
    setMergeDialogOpen(true);
  };

  const handleConfirmMerge = async () => {
    const b = mergeTargetBranch.trim();
    if (!b) {
      toast.error("Selecione a branch de destino.");
      return;
    }
    setIsMerging(true);
    try {
      const result = await window.electronAPI?.comb?.mergeIntoMain(comb.id, b);
      if (result?.success) {
        toast.success("Missão mergeada com sucesso");
        setMergeDialogOpen(false);
        onAction();
      } else {
        toast.error(result?.error ?? "Erro ao fazer merge");
      }
    } finally {
      setIsMerging(false);
    }
  };

  const handleCommit = async (message: string) => {
    if (!comb.worktreePath || !window.electronAPI?.git) return;
    try {
      const ok = await window.electronAPI.git.commit(
        comb.worktreePath,
        message,
      );
      if (ok) {
        toast.success("Commit realizado");
        const result = await window.electronAPI?.comb?.getDiffs(comb.id);
        if (result?.success) {
          setDiffs({
            loading: false,
            files: result.files,
            summary: result.summary,
          });
        }
      } else {
        toast.error("Falha ao commitar");
        throw new Error("Falha ao commitar");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro desconhecido";
      toast.error(msg);
      throw e;
    }
  };

  const handlePush = async () => {
    if (!comb.worktreePath || !window.electronAPI?.git?.push) return;
    setIsApplying(true);
    try {
      const result = await window.electronAPI.git.push(comb.worktreePath);
      if (result?.success) {
        toast.success("Push enviado");
      } else {
        toast.error(result?.error ?? "Falha ao enviar push");
      }
    } finally {
      setIsApplying(false);
    }
  };

  useEffect(() => {
    if (!commitDialogOpen || !comb.worktreePath || !window.electronAPI?.git) {
      if (!commitDialogOpen) setCommitDialogStatus(null);
      return;
    }
    window.electronAPI.git
      .getStatus(comb.worktreePath)
      .then((s) => setCommitDialogStatus(s))
      .catch(() => setCommitDialogStatus(null));
  }, [commitDialogOpen, comb.worktreePath]);

  if (!comb.worktreePath) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <p className="text-sm text-muted-foreground">
          Worktree ainda não criada.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-border px-4 py-2">
        <div className="flex items-center gap-3">
          <h4 className="text-sm font-semibold">Review</h4>
          {diffs.summary && (
            <span className="text-xs text-muted-foreground">
              {diffs.summary.changedFiles} arquivo(s), +
              {diffs.summary.insertions} -{diffs.summary.deletions}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setCommitDialogOpen(true)}
            disabled={diffs.files.length === 0}
          >
            <FileCode className="mr-1 h-3 w-3" />
            Commit
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={handlePush}
            disabled={isApplying}
          >
            <Upload className="mr-1 h-3 w-3" />
            Push
          </Button>
          <Button
            variant="default"
            size="sm"
            onClick={handleMergeClick}
            disabled={isMerging}
          >
            {isMerging ? (
              <Loader2 className="mr-1 h-3 w-3 animate-spin" />
            ) : (
              <GitMerge className="mr-1 h-3 w-3" />
            )}
            Merge
          </Button>
        </div>
      </div>

      <Dialog open={mergeDialogOpen} onOpenChange={setMergeDialogOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Merge na branch de destino</DialogTitle>
            <DialogDescription>
              O branch do worktree desta Missão será integrado na branch
              escolhida do repositório principal (não no worktree).
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2 py-2">
            <label className="text-sm font-medium">Branch de destino</label>
            {mergeBranches.length > 0 ? (
              <Select
                value={mergeTargetBranch}
                onValueChange={setMergeTargetBranch}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Selecione a branch" />
                </SelectTrigger>
                <SelectContent>
                  {mergeBranches.map((br) => (
                    <SelectItem key={br} value={br}>
                      {br}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input
                placeholder="main"
                value={mergeTargetBranch}
                onChange={(e) => setMergeTargetBranch(e.target.value)}
              />
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setMergeDialogOpen(false)}>
              Cancelar
            </Button>
            <Button onClick={handleConfirmMerge} disabled={isMerging}>
              {isMerging && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Merge
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ScrollArea className="flex-1">
        {diffs.loading ? (
          <div className="flex items-center justify-center p-8">
            <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : diffs.error ? (
          <div className="p-4 text-sm text-destructive">{diffs.error}</div>
        ) : diffs.files.length === 0 ? (
          <div className="p-8 text-center text-sm text-muted-foreground">
            Nenhuma alteração detectada neste worktree.
          </div>
        ) : (
          <div className="space-y-4 p-4">
            {diffs.files.map((file) => (
              <div key={file.path} className="rounded-lg border border-border">
                <div className="flex items-center gap-2 border-b border-border px-3 py-2">
                  <FileCode className="h-3.5 w-3.5 text-muted-foreground" />
                  <span className="text-sm font-mono">{file.path}</span>
                  <Badge variant="outline" className="text-[10px]">
                    {file.status}
                  </Badge>
                </div>
                <DiffCodeBlock content={file.diff} />
              </div>
            ))}
          </div>
        )}
      </ScrollArea>

      <CommitDialog
        open={commitDialogOpen}
        onOpenChange={setCommitDialogOpen}
        onCommit={handleCommit}
        defaultMessage={`Changes from mission: ${comb.name}`}
        projectPath={comb.worktreePath ?? ""}
        status={commitDialogStatus}
      />
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
  const [projectPickerOpen, setProjectPickerOpen] = useState(false);

  const sortedProjects = useMemo(
    () =>
      [...projects].sort((a, b) => {
        const da = a.lastOpenedAt?.getTime() ?? a.createdAt.getTime();
        const db = b.lastOpenedAt?.getTime() ?? b.createdAt.getTime();
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
  } = useCombs(selectedProjectId ?? undefined);

  const activeComb = useMemo(
    () =>
      activeCombId ? (combs.find((c) => c.id === activeCombId) ?? null) : null,
    [activeCombId, combs],
  );

  const {
    panes,
    refresh: refreshPanes,
    create: createPane,
    remove: removePane,
  } = usePanes(activeCombId ?? undefined);

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
        const da = a.lastOpenedAt?.getTime() ?? a.createdAt.getTime();
        const db = b.lastOpenedAt?.getTime() ?? b.createdAt.getTime();
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

  // Ensure worktree when comb becomes active
  useEffect(() => {
    if (!activeComb || activeComb.worktreePath) return;
    if (!window.electronAPI?.comb?.ensureWorktree) return;
    window.electronAPI.comb.ensureWorktree(activeComb.id).then((result) => {
      if (result.success) {
        refreshCombs();
      } else if (result.error) {
        toast.error(`Worktree: ${result.error}`);
      }
    });
  }, [activeComb?.id, activeComb?.worktreePath, refreshCombs]);

  const handleProjectChange = (projectId: string) => {
    setSelectedProjectId(projectId);
    setActiveCombId(null);
    setActiveMainTab("panes");
  };

  const handleCombCreated = (comb: Comb) => {
    setActiveCombId(comb.id);
    setShowSettings(false);
    refreshCombs();
  };

  const handleAddTerminal = async () => {
    if (!activeCombId) return;
    try {
      await createPane({ combId: activeCombId, type: "term" });
      refreshPanes();
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
  };

  const handleRemoveComb = async (combId: string) => {
    const confirmed = await confirmDialog({
      title: "Remover Missão?",
      description: "O worktree e todos os panes serão removidos.",
      confirmLabel: "Remover",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    if (window.electronAPI?.comb?.discard) {
      const result = await window.electronAPI.comb.discard(combId);
      if (!result.success && result.error) toast.error(result.error);
    }
    if (window.db?.combs) await window.db.combs.delete(combId);
    if (activeCombId === combId) setActiveCombId(null);
    refreshCombs();
    toast.success("Missão removida");
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
      <div className="electron-drag fixed top-0 left-0 right-0 h-8 z-50" />

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
                className="electron-no-drag flex w-full items-center justify-between rounded-md border border-sidebar-border bg-sidebar-accent/30 px-3 py-2 text-sm transition-colors hover:bg-sidebar-accent/50"
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
        <div className="flex items-center justify-between px-4 py-2">
          <span className="text-xs font-medium uppercase tracking-wider text-sidebar-foreground/50">
            Missões
          </span>
          <Button
            variant="ghost"
            size="icon"
            className="electron-no-drag h-6 w-6 text-sidebar-foreground/50 hover:bg-sidebar-accent hover:text-sidebar-foreground"
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
                Nenhuma Missão ainda
              </p>
              <Button
                variant="ghost"
                size="sm"
                className="electron-no-drag mt-2 text-xs"
                onClick={() => setNewCombOpen(true)}
              >
                <Plus className="mr-1 h-3 w-3" />
                Criar primeira Missão
              </Button>
            </div>
          ) : (
            <div className="flex flex-col gap-1 py-1">
              {combs.map((comb) => {
                const isActive = comb.id === activeCombId;
                return (
                  <button
                    key={comb.id}
                    type="button"
                    onClick={() => {
                      setActiveCombId(comb.id);
                      setShowSettings(false);
                    }}
                    className={`electron-no-drag group flex w-full cursor-pointer gap-1.5 rounded-lg px-3 py-2 text-left transition-colors ${
                      isActive
                        ? "bg-sidebar-accent text-sidebar-accent-foreground"
                        : "text-sidebar-foreground/70 hover:bg-sidebar-accent/50 hover:text-sidebar-foreground"
                    }`}
                  >
                    <div className="min-w-0 flex-1 flex flex-col">
                      <span className="truncate text-sm font-medium">
                        {comb.name}
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
                  </button>
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
            className="electron-no-drag w-full justify-start gap-3 text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground"
            onClick={() => {
              setShowSettings(true);
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
        {showSettings ? (
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

            {/* Panes or Review */}
            <div className="min-h-0 flex-1 overflow-hidden">
              {activeMainTab === "panes" ? (
                panes.length === 0 ? (
                  <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
                    <Terminal className="h-12 w-12 text-muted-foreground/30" />
                    <p className="text-sm text-muted-foreground">
                      Nenhum pane aberto nesta Missão
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
                    className="grid h-full min-h-0 min-w-0 gap-1 overflow-hidden p-1 [grid-auto-rows:minmax(0,1fr)]"
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
                )
              ) : (
                <CombReviewPanel
                  comb={activeComb}
                  mainProjectPath={selectedProject?.path}
                  onAction={() => refreshCombs()}
                />
              )}
            </div>
          </>
        ) : selectedProjectId ? (
          <div className="flex h-full flex-col items-center justify-center gap-4 p-8 mt-8">
            <FolderGit2 className="h-16 w-16 text-muted-foreground/20" />
            <div className="text-center">
              <h3 className="text-lg font-medium">Comece criando uma Missão</h3>
              <p className="mt-1 text-sm text-muted-foreground">
                Cada Missão cria uma worktree isolada onde seus agents e
                terminais compartilham o mesmo espaço.
              </p>
            </div>
            <Button onClick={() => setNewCombOpen(true)}>
              <Plus className="mr-2 h-4 w-4" />
              Nova Missão
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
        />
      )}

      {activeCombId && (
        <NewAgentPaneDialog
          open={newAgentOpen}
          onOpenChange={setNewAgentOpen}
          combId={activeCombId}
          providers={providers}
          onCreate={() => refreshPanes()}
        />
      )}

      <AddProjectDialog
        open={addProjectOpen}
        onOpenChange={setAddProjectOpen}
      />
    </div>
  );
}
