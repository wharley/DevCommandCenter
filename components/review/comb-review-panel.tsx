"use client";

import React, {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  AlertTriangle,
  Check,
  Clock,
  FileCode,
  GitMerge,
  Loader2,
  MoreHorizontal,
  Upload,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { DiffCodeBlock } from "@/components/diff-code-block";
import { CommitDialog } from "@/components/dialogs/commit-dialog";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import type { Comb } from "@/lib/database/types";
import type { GitStatus } from "@/types/electron";
import { toast } from "sonner";

type ReviewFlag = "ok" | "later" | "suspicious";

type TrailEntry = { id: string; at: number; message: string };

const reviewFlagsKey = (combId: string) => `dcc-review-flags-${combId}`;
const reviewIncludedKey = (combId: string) => `dcc-review-included-${combId}`;
const reviewTrailKey = (combId: string) => `dcc-review-trail-${combId}`;

function loadReviewJson<T>(key: string): T | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

function saveReviewJson(key: string, value: unknown): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* quota */
  }
}

export function CombReviewPanel({
  comb,
  mainProjectPath,
  onAction,
}: {
  comb: Comb;
  mainProjectPath?: string;
  onAction: () => void;
}) {
  const { confirmDialog } = useConfirmDialog();

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

  const [fileFlags, setFileFlags] = useState<Record<string, ReviewFlag>>({});
  const [included, setIncluded] = useState<Record<string, boolean>>({});
  const [trail, setTrail] = useState<TrailEntry[]>([]);

  const [targetBranch, setTargetBranch] = useState("");
  const [branchList, setBranchList] = useState<string[]>([]);

  const [isApplyingPatch, setIsApplyingPatch] = useState(false);
  const [isMerging, setIsMerging] = useState(false);
  const [isPushing, setIsPushing] = useState(false);

  const [commitDialogOpen, setCommitDialogOpen] = useState(false);
  const [commitDialogStatus, setCommitDialogStatus] =
    useState<GitStatus | null>(null);

  const [mergeDialogOpen, setMergeDialogOpen] = useState(false);
  const [applyCommitDialogOpen, setApplyCommitDialogOpen] = useState(false);
  const [applyCommitMessage, setApplyCommitMessage] = useState("");

  const filePathsKey = useMemo(
    () => diffs.files.map((f) => f.path).sort().join("\0"),
    [diffs.files],
  );

  const addTrail = useCallback(
    (message: string) => {
      const entry: TrailEntry = {
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
        at: Date.now(),
        message,
      };
      setTrail((prev) => {
        const next = [...prev, entry].slice(-20);
        saveReviewJson(reviewTrailKey(comb.id), next);
        return next;
      });
    },
    [comb.id],
  );

  const loadDiffs = useCallback(async () => {
    if (!comb.worktreePath || !window.electronAPI?.comb?.getDiffs) return;
    setDiffs((prev) => ({ ...prev, loading: true, error: undefined }));
    const result = await window.electronAPI.comb.getDiffs(comb.id);
    if (result.success) {
      setDiffs({
        loading: false,
        files: result.files,
        summary: result.summary,
      });
      addTrail(
        `Alterações carregadas — ${comb.name}: ${result.files.length} arquivo(s)`,
      );
    } else {
      setDiffs({
        loading: false,
        error: result.error,
        files: [],
        summary: null,
      });
    }
  }, [comb.id, comb.worktreePath, comb.name, addTrail]);

  useEffect(() => {
    void loadDiffs();
  }, [loadDiffs]);

  useEffect(() => {
    const t = loadReviewJson<TrailEntry[]>(reviewTrailKey(comb.id));
    setTrail(Array.isArray(t) ? t.slice(-20) : []);
  }, [comb.id]);

  useEffect(() => {
    if (!mainProjectPath?.trim()) return;
    const git = window.electronAPI?.git;
    if (!git?.getLocalBranches || !git?.getCurrentBranch) return;
    let cancelled = false;
    Promise.all([
      git.getLocalBranches(mainProjectPath),
      git.getCurrentBranch(mainProjectPath),
    ]).then(([branches, current]) => {
      if (cancelled) return;
      const list = branches ?? [];
      setBranchList(list);
      const c = (current ?? "").trim();
      setTargetBranch((prev) => (prev.trim() ? prev : c || list[0] || "main"));
    });
    return () => {
      cancelled = true;
    };
  }, [mainProjectPath, comb.id]);

  useEffect(() => {
    if (!diffs.files.length) {
      setFileFlags({});
      setIncluded({});
      return;
    }
    const paths = new Set(diffs.files.map((f) => f.path));
    const storedFlags =
      loadReviewJson<Record<string, ReviewFlag>>(reviewFlagsKey(comb.id)) ??
      {};
    const storedInc =
      loadReviewJson<Record<string, boolean>>(reviewIncludedKey(comb.id)) ??
      {};

    setFileFlags((prev) => {
      const next: Record<string, ReviewFlag> = {};
      for (const p of paths) {
        next[p] = storedFlags[p] ?? prev[p] ?? "ok";
      }
      return next;
    });
    setIncluded((prev) => {
      const next: Record<string, boolean> = {};
      for (const p of paths) {
        next[p] = storedInc[p] ?? prev[p] ?? true;
      }
      return next;
    });
  }, [comb.id, filePathsKey, diffs.files.length]);

  useEffect(() => {
    if (Object.keys(fileFlags).length === 0) return;
    saveReviewJson(reviewFlagsKey(comb.id), fileFlags);
  }, [comb.id, fileFlags]);

  useEffect(() => {
    if (Object.keys(included).length === 0) return;
    saveReviewJson(reviewIncludedKey(comb.id), included);
  }, [comb.id, included]);

  const summaryCounts = useMemo(() => {
    let ok = 0;
    let later = 0;
    let suspicious = 0;
    for (const f of diffs.files) {
      const fl = fileFlags[f.path] ?? "ok";
      if (fl === "ok") ok++;
      else if (fl === "later") later++;
      else suspicious++;
    }
    return { ok, later, suspicious };
  }, [diffs.files, fileFlags]);

  const includedPaths = useMemo(
    () =>
      diffs.files.filter((f) => included[f.path] !== false).map((f) => f.path),
    [diffs.files, included],
  );

  const hasIncludedFiles = includedPaths.length > 0;

  const handleMergeClick = () => {
    if (!mainProjectPath?.trim()) {
      toast.error("Caminho do repositório principal indisponível.");
      return;
    }
    setMergeDialogOpen(true);
  };

  const handleConfirmMerge = async () => {
    const b = targetBranch.trim();
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
        addTrail(`Merge na branch ${b} concluído`);
        onAction();
      } else {
        toast.error(result?.error ?? "Erro ao fazer merge");
      }
    } finally {
      setIsMerging(false);
    }
  };

  const handleCommitWorktree = async (message: string) => {
    if (!comb.worktreePath || !window.electronAPI?.git) return;
    const paths = includedPaths.length > 0 ? includedPaths : undefined;
    try {
      const ok = await window.electronAPI.git.commit(
        comb.worktreePath,
        message,
        paths,
      );
      if (ok) {
        toast.success("Commit realizado na worktree");
        addTrail(
          `Commit na worktree: ${message.slice(0, 60)}${message.length > 60 ? "…" : ""}`,
        );
        await loadDiffs();
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
    setIsPushing(true);
    try {
      const result = await window.electronAPI.git.push(comb.worktreePath);
      if (result?.success) {
        toast.success("Push enviado");
        addTrail("Push do branch da worktree enviado");
      } else {
        toast.error(result?.error ?? "Falha ao enviar push");
      }
    } finally {
      setIsPushing(false);
    }
  };

  const runApplyPatch = async (opts: {
    commit: boolean;
    message?: string;
  }): Promise<boolean> => {
    if (!mainProjectPath?.trim()) {
      toast.error("Caminho do repositório principal indisponível.");
      return false;
    }
    const b = targetBranch.trim();
    if (!b) {
      toast.error("Selecione a branch de destino.");
      return false;
    }
    if (!hasIncludedFiles) {
      toast.error("Selecione pelo menos um arquivo para aplicar.");
      return false;
    }
    const api = window.electronAPI?.comb?.applyPatch;
    if (!api) {
      toast.error("Aplicar patch indisponível.");
      return false;
    }
    setIsApplyingPatch(true);
    try {
      const result = await api(comb.id, b, {
        includeFiles: includedPaths,
        commit: opts.commit,
        message:
          opts.message?.trim() || `Apply from mission: ${comb.name}`,
      });
      if (result?.success) {
        toast.success(
          opts.commit
            ? "Patch aplicado e commitado no repositório principal"
            : "Patch aplicado no repositório principal",
        );
        addTrail(
          opts.commit
            ? `Patch aplicado com commit em ${b} às ${new Date().toLocaleTimeString()}`
            : `Patch aplicado em ${b} às ${new Date().toLocaleTimeString()}`,
        );
        onAction();
        await loadDiffs();
        return true;
      }
      toast.error(result?.error ?? "Falha ao aplicar patch");
      return false;
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro ao aplicar patch";
      toast.error(msg);
      return false;
    } finally {
      setIsApplyingPatch(false);
    }
  };

  const handleDiscardWorktree = async () => {
    if (!comb.worktreePath || !window.electronAPI?.git?.reset) return;
    const ok = await confirmDialog({
      title: "Descartar alterações locais?",
      description:
        "Será executado git reset --hard nesta worktree. Isto não remove a Missão.",
      confirmLabel: "Descartar",
      cancelLabel: "Cancelar",
    });
    if (!ok) return;
    const result = await window.electronAPI.git.reset(
      comb.worktreePath,
      "HEAD",
    );
    if (result.success) {
      toast.success("Alterações locais descartadas");
      addTrail("Alterações locais descartadas (reset --hard na worktree)");
      await loadDiffs();
    } else {
      toast.error(result.error ?? "Falha ao descartar");
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

  useEffect(() => {
    if (applyCommitDialogOpen) {
      setApplyCommitMessage(`Apply from mission: ${comb.name}`);
    }
  }, [applyCommitDialogOpen, comb.name]);

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
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 space-y-3 border-b border-border px-4 py-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="space-y-1">
            <h4 className="text-sm font-semibold">Review</h4>
            {diffs.summary && diffs.files.length > 0 && (
              <p className="text-xs text-muted-foreground">
                {diffs.files.length} arquivo(s) alterado(s): {summaryCounts.ok}{" "}
                OK, {summaryCounts.later} rever depois,{" "}
                {summaryCounts.suspicious} suspeito(s) · +
                {diffs.summary.insertions} −{diffs.summary.deletions}
              </p>
            )}
            {diffs.summary && diffs.files.length === 0 && !diffs.loading && (
              <p className="text-xs text-muted-foreground">
                Nenhuma alteração pendente
              </p>
            )}
          </div>
          <div className="flex min-w-[200px] flex-col gap-1">
            <Label className="text-[10px] uppercase text-muted-foreground">
              Branch de destino (repo principal)
            </Label>
            {branchList.length > 0 ? (
              <Select value={targetBranch} onValueChange={setTargetBranch}>
                <SelectTrigger className="h-8 text-xs">
                  <SelectValue placeholder="Branch" />
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
                className="h-8 text-xs"
                placeholder="main"
                value={targetBranch}
                onChange={(e) => setTargetBranch(e.target.value)}
              />
            )}
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            disabled={
              isApplyingPatch || !hasIncludedFiles || diffs.files.length === 0
            }
            onClick={() => void runApplyPatch({ commit: false })}
          >
            {isApplyingPatch ? (
              <Loader2 className="mr-1 h-3 w-3 animate-spin" />
            ) : null}
            Aplicar
          </Button>
          <Button
            variant="secondary"
            size="sm"
            disabled={
              isApplyingPatch || !hasIncludedFiles || diffs.files.length === 0
            }
            onClick={() => setApplyCommitDialogOpen(true)}
          >
            Aplicar + Commit
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={diffs.files.length === 0}
            onClick={() => void handleDiscardWorktree()}
          >
            Descartar
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="sm" className="gap-1">
                <MoreHorizontal className="h-3 w-3" />
                Mais
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem
                disabled={diffs.files.length === 0}
                onClick={() => setCommitDialogOpen(true)}
              >
                <FileCode className="mr-2 h-3 w-3" />
                Commit na worktree…
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={isPushing || !comb.worktreePath}
                onClick={() => void handlePush()}
              >
                <Upload className="mr-2 h-3 w-3" />
                Push (worktree)
              </DropdownMenuItem>
              <DropdownMenuItem disabled={isMerging} onClick={handleMergeClick}>
                <GitMerge className="mr-2 h-3 w-3" />
                Integrar branch inteiro (merge)…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      <Dialog open={mergeDialogOpen} onOpenChange={setMergeDialogOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Merge na branch de destino</DialogTitle>
            <DialogDescription>
              Integra o branch completo da Missão na branch escolhida do
              repositório principal. Não usa os checkboxes por arquivo.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2 py-2">
            <label className="text-sm font-medium">Branch de destino</label>
            {branchList.length > 0 ? (
              <Select value={targetBranch} onValueChange={setTargetBranch}>
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
                placeholder="main"
                value={targetBranch}
                onChange={(e) => setTargetBranch(e.target.value)}
              />
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setMergeDialogOpen(false)}>
              Cancelar
            </Button>
            <Button
              onClick={() => void handleConfirmMerge()}
              disabled={isMerging}
            >
              {isMerging && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Merge
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={applyCommitDialogOpen}
        onOpenChange={setApplyCommitDialogOpen}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Commit no repositório principal</DialogTitle>
            <DialogDescription>
              O patch dos arquivos selecionados será aplicado em{" "}
              <span className="font-mono">{targetBranch || "…"}</span> e em
              seguida commitado lá.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2 py-2">
            <Label htmlFor="apply-commit-msg">Mensagem</Label>
            <Textarea
              id="apply-commit-msg"
              value={applyCommitMessage}
              onChange={(e) => setApplyCommitMessage(e.target.value)}
              rows={3}
              className="text-sm"
            />
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setApplyCommitDialogOpen(false)}
            >
              Cancelar
            </Button>
            <Button
              onClick={() => {
                void runApplyPatch({
                  commit: true,
                  message: applyCommitMessage,
                }).then((ok) => {
                  if (ok) setApplyCommitDialogOpen(false);
                });
              }}
              disabled={isApplyingPatch}
            >
              {isApplyingPatch && (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              )}
              Aplicar e commitar
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ScrollArea className="min-h-0 flex-1">
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
                <div className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2">
                  <Checkbox
                    checked={included[file.path] !== false}
                    onCheckedChange={(c) =>
                      setIncluded((prev) => ({
                        ...prev,
                        [file.path]: c === true,
                      }))
                    }
                    aria-label={`Incluir ${file.path} no patch`}
                  />
                  <ToggleGroup
                    type="single"
                    value={fileFlags[file.path] ?? "ok"}
                    onValueChange={(v) => {
                      if (!v) return;
                      setFileFlags((prev) => ({
                        ...prev,
                        [file.path]: v as ReviewFlag,
                      }));
                    }}
                    variant="outline"
                    size="sm"
                    className="shrink-0"
                  >
                    <ToggleGroupItem value="ok" aria-label="OK">
                      <Check className="h-3.5 w-3.5 text-emerald-500" />
                    </ToggleGroupItem>
                    <ToggleGroupItem value="later" aria-label="Rever depois">
                      <Clock className="h-3.5 w-3.5 text-amber-500" />
                    </ToggleGroupItem>
                    <ToggleGroupItem value="suspicious" aria-label="Suspeito">
                      <AlertTriangle className="h-3.5 w-3.5 text-rose-500" />
                    </ToggleGroupItem>
                  </ToggleGroup>
                  <FileCode className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate font-mono text-sm">
                    {file.path}
                  </span>
                  <Badge variant="outline" className="shrink-0 text-[10px]">
                    {file.status}
                  </Badge>
                </div>
                <DiffCodeBlock content={file.diff} />
              </div>
            ))}
          </div>
        )}
      </ScrollArea>

      <div className="shrink-0 border-t border-border bg-muted/20">
        <div className="px-3 py-2">
          <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
            Trilha
          </p>
          <ScrollArea className="h-24 pr-2">
            <ul className="space-y-1.5 text-xs text-muted-foreground">
              {trail.length === 0 ? (
                <li className="italic opacity-70">
                  Eventos de revisão aparecem aqui.
                </li>
              ) : (
                [...trail].reverse().map((e) => (
                  <li key={e.id} className="leading-snug">
                    <span className="font-mono text-[10px] opacity-70">
                      {new Date(e.at).toLocaleTimeString()}
                    </span>{" "}
                    {e.message}
                  </li>
                ))
              )}
            </ul>
          </ScrollArea>
        </div>
      </div>

      <CommitDialog
        open={commitDialogOpen}
        onOpenChange={setCommitDialogOpen}
        onCommit={handleCommitWorktree}
        defaultMessage={`Changes from mission: ${comb.name}`}
        projectPath={comb.worktreePath ?? ""}
        status={commitDialogStatus}
      />
    </div>
  );
}
