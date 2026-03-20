"use client";

import React, {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  FileCode,
  GitBranch,
  GitMerge,
  Loader2,
  Plus,
  Terminal,
  Trash2,
  Bot,
  FolderGit2,
  Upload,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { DiffCodeBlock } from "@/components/diff-code-block";
import { CommitDialog } from "@/components/dialogs/commit-dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import { useCombs, usePanes, useProviders } from "@/hooks/use-data";
import { useProjectWorkspaceContext } from "@/src/pages/ProjectWorkspacePage";
import type { Comb, Pane, Provider } from "@/lib/database/types";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import { toast } from "sonner";

const CLI_PROVIDER_TYPES = ["codex", "claude-code", "gemini", "cursor"] as const;

function isCliProviderType(
  type: string,
): type is (typeof CLI_PROVIDER_TYPES)[number] {
  return CLI_PROVIDER_TYPES.includes(type as (typeof CLI_PROVIDER_TYPES)[number]);
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
    case "active": return "Ativo";
    case "ready_for_review": return "Revisão";
    case "applied": return "Aplicado";
    case "discarded": return "Descartado";
    case "archived": return "Arquivado";
    case "error": return "Erro";
    default: return comb.status;
  }
}

function getCombStatusVariant(status: Comb["status"]) {
  switch (status) {
    case "active": return "default" as const;
    case "ready_for_review": return "secondary" as const;
    case "applied": return "outline" as const;
    case "discarded": return "outline" as const;
    case "error": return "destructive" as const;
    default: return "outline" as const;
  }
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
    return () => { cancelled = true; };
  }, [open, projectPath]);

  const handleCreate = async () => {
    if (!name.trim()) {
      toast.error("Nome do Comb é obrigatório");
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
      toast.success("Comb criado");
      onCreate(comb);
      onOpenChange(false);
      setName("");
      setDescription("");
    } catch {
      toast.error("Falha ao criar Comb");
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Novo Comb</DialogTitle>
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
              placeholder="O que este Comb faz..."
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
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
                    <SelectItem key={b} value={b}>{b}</SelectItem>
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
            Criar Comb
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ==========================================
// New Pane Dialog (Agent)
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
            Abre um agente CLI neste Comb.
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
                  <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
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
            <label className="text-sm font-medium">Prompt inicial (opcional)</label>
            <Textarea
              placeholder="Instrução para o agente..."
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={3}
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
  const args = pane.type === "agent" && pane.initialPrompt
    ? [pane.initialPrompt]
    : [];

  const label = pane.type === "agent"
    ? (pane.title ?? provider?.name ?? "Agent")
    : (pane.title ?? "Terminal");

  if (!cwd) {
    return (
      <div className="flex h-full flex-col items-center justify-center rounded-lg border border-border bg-card p-4">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        <p className="mt-2 text-sm text-muted-foreground">Preparando worktree...</p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
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
      <div className="min-h-0 flex-1">
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
  onAction,
}: {
  comb: Comb;
  onAction: () => void;
}) {
  const [diffs, setDiffs] = useState<{
    loading: boolean;
    error?: string;
    files: Array<{ path: string; status: string; diff: string }>;
    summary: { changedFiles: number; insertions: number; deletions: number } | null;
  }>({ loading: false, files: [], summary: null });
  const [isApplying, setIsApplying] = useState(false);
  const [isMerging, setIsMerging] = useState(false);
  const [commitDialogOpen, setCommitDialogOpen] = useState(false);
  const [commitDialogStatus, setCommitDialogStatus] = useState<import("@/types/electron").GitStatus | null>(null);
  const { confirmDialog } = useConfirmDialog();

  useEffect(() => {
    if (!comb.worktreePath || !window.electronAPI?.comb?.getDiffs) return;
    setDiffs((prev) => ({ ...prev, loading: true, error: undefined }));
    window.electronAPI.comb.getDiffs(comb.id).then((result) => {
      if (result.success) {
        setDiffs({ loading: false, files: result.files, summary: result.summary });
      } else {
        setDiffs({ loading: false, error: result.error, files: [], summary: null });
      }
    });
  }, [comb.id, comb.worktreePath]);

  const handleMerge = async () => {
    const confirmed = await confirmDialog({
      title: "Merge Comb na branch principal?",
      description: "As alterações do worktree serão integradas na branch ativa do projeto.",
      confirmLabel: "Merge",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    setIsMerging(true);
    try {
      const result = await window.electronAPI?.comb?.mergeIntoMain(comb.id);
      if (result?.success) {
        toast.success("Comb mergeado com sucesso");
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
      const ok = await window.electronAPI.git.commit(comb.worktreePath, message);
      if (ok) {
        toast.success("Commit realizado");
        // Reload diffs
        const result = await window.electronAPI?.comb?.getDiffs(comb.id);
        if (result?.success) {
          setDiffs({ loading: false, files: result.files, summary: result.summary });
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
        <p className="text-sm text-muted-foreground">Worktree ainda não criada.</p>
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
              {diffs.summary.changedFiles} arquivo(s), +{diffs.summary.insertions} -{diffs.summary.deletions}
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
            onClick={handleMerge}
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
        defaultMessage={`Changes from comb: ${comb.name}`}
        projectPath={comb.worktreePath ?? ""}
        status={commitDialogStatus}
      />
    </div>
  );
}

// ==========================================
// Main Page
// ==========================================
export default function ProjectCombsPage() {
  const { projectId, project, providers } = useProjectWorkspaceContext();
  const { combs, isLoading: combsLoading, refresh: refreshCombs } = useCombs(projectId);
  const { confirmDialog } = useConfirmDialog();

  const [activeCombId, setActiveCombId] = useState<string | null>(null);
  const [activeMainTab, setActiveMainTab] = useState<"panes" | "review">("panes");
  const [newCombOpen, setNewCombOpen] = useState(false);
  const [newAgentOpen, setNewAgentOpen] = useState(false);

  const activeComb = useMemo(
    () => (activeCombId ? combs.find((c) => c.id === activeCombId) ?? null : null),
    [activeCombId, combs],
  );

  const { panes, refresh: refreshPanes, create: createPane, remove: removePane } =
    usePanes(activeCombId ?? undefined);

  const providerById = useMemo(() => {
    const map = new Map<string, Provider>();
    for (const p of providers) map.set(p.id, p);
    return map;
  }, [providers]);

  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProviderType(p.type)),
    [providers],
  );

  // Auto-select first comb or restore from localStorage
  useEffect(() => {
    if (activeCombId && combs.some((c) => c.id === activeCombId)) return;
    const stored = localStorage.getItem(`dcc:project:${projectId}:activeComb`);
    if (stored && combs.some((c) => c.id === stored)) {
      setActiveCombId(stored);
      return;
    }
    if (combs.length > 0) {
      setActiveCombId(combs[0].id);
    }
  }, [combs, activeCombId, projectId]);

  useEffect(() => {
    if (activeCombId) {
      localStorage.setItem(`dcc:project:${projectId}:activeComb`, activeCombId);
    }
  }, [activeCombId, projectId]);

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

  const handleCombCreated = (comb: Comb) => {
    setActiveCombId(comb.id);
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
      title: "Remover Comb?",
      description: "O worktree e todos os panes serão removidos permanentemente.",
      confirmLabel: "Remover",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;

    if (window.electronAPI?.comb?.discard) {
      const result = await window.electronAPI.comb.discard(combId);
      if (!result.success && result.error) {
        toast.error(result.error);
      }
    }

    if (window.db?.combs) {
      await window.db.combs.delete(combId);
    }

    if (activeCombId === combId) {
      setActiveCombId(null);
    }
    refreshCombs();
    toast.success("Comb removido");
  };

  if (combsLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0">
      {/* ====== LEFT SIDEBAR: COMBS LIST ====== */}
      <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-card">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-sm font-semibold">Combs</h2>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => setNewCombOpen(true)}
          >
            <Plus className="h-4 w-4" />
          </Button>
        </div>

        <div className="flex-1 overflow-auto">
          {combs.length === 0 ? (
            <div className="p-4 text-center">
              <FolderGit2 className="mx-auto h-8 w-8 text-muted-foreground/40" />
              <p className="mt-2 text-sm text-muted-foreground">
                Nenhum Comb ainda
              </p>
              <Button
                variant="outline"
                size="sm"
                className="mt-3"
                onClick={() => setNewCombOpen(true)}
              >
                <Plus className="mr-1 h-3 w-3" />
                Criar primeiro Comb
              </Button>
            </div>
          ) : (
            <div className="space-y-1 p-2">
              {combs.map((comb) => {
                const isActive = comb.id === activeCombId;
                return (
                  <button
                    key={comb.id}
                    onClick={() => setActiveCombId(comb.id)}
                    className={`group flex w-full flex-col rounded-md px-3 py-2 text-left transition-colors ${
                      isActive
                        ? "bg-primary/10 text-primary"
                        : "hover:bg-muted/50"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="truncate text-sm font-medium">
                        {comb.name}
                      </span>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-5 w-5 opacity-0 group-hover:opacity-100"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleRemoveComb(comb.id);
                        }}
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                    </div>
                    <div className="mt-0.5 flex items-center gap-2">
                      <GitBranch className="h-3 w-3 text-muted-foreground" />
                      <span className="truncate text-xs text-muted-foreground">
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
                      {comb.updatedAt && (
                        <span className="text-[10px] text-muted-foreground">
                          {formatDistanceToNow(comb.updatedAt, {
                            addSuffix: true,
                            locale: ptBR,
                          })}
                        </span>
                      )}
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </aside>

      {/* ====== MAIN AREA: PANES GRID ====== */}
      <main className="flex min-h-0 flex-1 flex-col">
        {activeComb ? (
          <>
            {/* Toolbar */}
            <div className="flex items-center justify-between border-b border-border px-4 py-2">
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

            {/* Content: Panes or Review */}
            <div className="min-h-0 flex-1 overflow-hidden">
              {activeMainTab === "panes" ? (
                panes.length === 0 ? (
                  <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
                    <Terminal className="h-12 w-12 text-muted-foreground/30" />
                    <p className="text-sm text-muted-foreground">
                      Nenhum pane aberto neste Comb
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
                    className="grid h-full gap-1 p-1"
                    style={{
                      gridTemplateColumns: `repeat(${Math.min(panes.length, 3)}, 1fr)`,
                    }}
                  >
                    {panes.map((pane) => (
                      <PaneGridItem
                        key={pane.id}
                        pane={pane}
                        comb={activeComb}
                        provider={
                          pane.providerId
                            ? providerById.get(pane.providerId) ?? null
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
                  onAction={() => refreshCombs()}
                />
              )}
            </div>
          </>
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
            <FolderGit2 className="h-16 w-16 text-muted-foreground/20" />
            <div className="text-center">
              <h3 className="text-lg font-medium">Comece criando um Comb</h3>
              <p className="mt-1 text-sm text-muted-foreground">
                Cada Comb cria uma worktree isolada onde seus agents e terminais compartilham o mesmo espaço.
              </p>
            </div>
            <Button onClick={() => setNewCombOpen(true)}>
              <Plus className="mr-2 h-4 w-4" />
              Novo Comb
            </Button>
          </div>
        )}
      </main>

      {/* Dialogs */}
      <NewCombDialog
        open={newCombOpen}
        onOpenChange={setNewCombOpen}
        projectId={projectId}
        projectPath={project?.path}
        onCreate={handleCombCreated}
      />

      {activeCombId && (
        <NewAgentPaneDialog
          open={newAgentOpen}
          onOpenChange={setNewAgentOpen}
          combId={activeCombId}
          providers={providers}
          onCreate={() => refreshPanes()}
        />
      )}
    </div>
  );
}
