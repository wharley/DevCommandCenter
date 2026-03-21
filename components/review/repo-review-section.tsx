"use client";

import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  AlertTriangle,
  Check,
  ChevronDown,
  Clock,
  FileCode,
  GitMerge,
  Loader2,
  Upload,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { extractContextTokens } from "@/lib/review/extract-context-tokens";
import type { GitStatus } from "@/types/electron";
import { toast } from "sonner";

export type ReviewFlag = "ok" | "later" | "suspicious";

type TrailEntry = { id: string; at: number; message: string };

const reviewFlagsKey = (storageKey: string) =>
  `dcc-review-flags-${storageKey}`;
const reviewIncludedKey = (storageKey: string) =>
  `dcc-review-included-${storageKey}`;
const reviewTrailKey = (storageKey: string) =>
  `dcc-review-trail-${storageKey}`;

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

export type RepoReviewSectionProps = {
  /** Identificador do target (para índice de tokens cruzados). */
  targetId: string;
  /** Chave estável para flags/trilha (ex.: `${combId}::${targetId}`). */
  storageKey: string;
  title: string;
  subtitle?: string;
  missionName: string;
  mainProjectPath: string;
  worktreePath: string;
  /** Quando definido e useCombWorktreeApis=true, merge/patch usam IPC comb.* */
  combId?: string | null;
  useCombWorktreeApis?: boolean;
  onAction: () => void;
  onStatsChange?: (s: {
    ok: number;
    later: number;
    suspicious: number;
    fileCount: number;
  }) => void;
  onFileTokens?: (
    tid: string,
    filePath: string,
    tokens: string[],
  ) => void;
  /** Trilha mais baixa quando há vários targets */
  compact?: boolean;
};

export function RepoReviewSection({
  targetId,
  storageKey,
  title,
  subtitle,
  missionName,
  mainProjectPath,
  worktreePath,
  combId,
  useCombWorktreeApis = false,
  onAction,
  onStatsChange,
  onFileTokens,
  compact = false,
}: RepoReviewSectionProps) {
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

  const [mainRepoStatus, setMainRepoStatus] = useState<GitStatus | null>(null);
  const [mainRepoStatusLoading, setMainRepoStatusLoading] = useState(false);

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
        saveReviewJson(reviewTrailKey(storageKey), next);
        return next;
      });
    },
    [storageKey],
  );

  const loadDiffs = useCallback(async () => {
    if (!worktreePath?.trim() || !window.electronAPI?.git?.getReviewDiffs) return;
    setDiffs((prev) => ({ ...prev, loading: true, error: undefined }));
    const result = await window.electronAPI.git.getReviewDiffs(worktreePath);
    if (result.success) {
      setDiffs({
        loading: false,
        files: result.files,
        summary: result.summary,
      });
      addTrail(
        `Alterações carregadas — ${title}: ${result.files.length} arquivo(s)`,
      );
    } else {
      setDiffs({
        loading: false,
        error: result.error ?? "Erro ao carregar diffs",
        files: [],
        summary: null,
      });
    }
  }, [worktreePath, title, addTrail]);

  useEffect(() => {
    void loadDiffs();
  }, [loadDiffs]);

  useEffect(() => {
    const t = loadReviewJson<TrailEntry[]>(reviewTrailKey(storageKey));
    setTrail(Array.isArray(t) ? t.slice(-20) : []);
  }, [storageKey]);

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
  }, [mainProjectPath, storageKey]);

  useEffect(() => {
    if (!diffs.files.length) {
      setFileFlags({});
      setIncluded({});
      return;
    }
    const paths = new Set(diffs.files.map((f) => f.path));
    const storedFlags =
      loadReviewJson<Record<string, ReviewFlag>>(
        reviewFlagsKey(storageKey),
      ) ?? {};
    const storedInc =
      loadReviewJson<Record<string, boolean>>(
        reviewIncludedKey(storageKey),
      ) ?? {};

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
  }, [storageKey, filePathsKey, diffs.files.length]);

  useEffect(() => {
    if (Object.keys(fileFlags).length === 0) return;
    saveReviewJson(reviewFlagsKey(storageKey), fileFlags);
  }, [storageKey, fileFlags]);

  useEffect(() => {
    if (Object.keys(included).length === 0) return;
    saveReviewJson(reviewIncludedKey(storageKey), included);
  }, [storageKey, included]);

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

  /** Ref evita loop infinito: o pai costuma passar inline `(s) => handleStats(id, s)` (nova ref a cada render). */
  const onStatsChangeRef = useRef(onStatsChange);
  onStatsChangeRef.current = onStatsChange;

  useEffect(() => {
    const cb = onStatsChangeRef.current;
    if (!cb) return;
    cb({
      ...summaryCounts,
      fileCount: diffs.files.length,
    });
  }, [
    summaryCounts.ok,
    summaryCounts.later,
    summaryCounts.suspicious,
    diffs.files.length,
  ]);

  const onFileTokensRef = useRef(onFileTokens);
  onFileTokensRef.current = onFileTokens;

  useEffect(() => {
    if (!diffs.files.length) return;
    const fn = onFileTokensRef.current;
    if (!fn) return;
    for (const f of diffs.files) {
      const tokens = extractContextTokens(f.diff);
      if (tokens.length > 0) {
        fn(targetId, f.path, tokens);
      }
    }
  }, [diffs.files, targetId]);

  const includedPaths = useMemo(
    () =>
      diffs.files.filter((f) => included[f.path] !== false).map((f) => f.path),
    [diffs.files, included],
  );

  const hasIncludedFiles = includedPaths.length > 0;

  const canMergeComb = Boolean(
    useCombWorktreeApis && combId && window.electronAPI?.comb?.mergeIntoMain,
  );

  const refreshMainRepoStatus = useCallback(async () => {
    if (!mainProjectPath?.trim() || !window.electronAPI?.git?.getStatus) {
      setMainRepoStatus(null);
      return;
    }
    setMainRepoStatusLoading(true);
    try {
      const s = await window.electronAPI.git.getStatus(mainProjectPath);
      setMainRepoStatus(s);
    } catch {
      setMainRepoStatus(null);
    } finally {
      setMainRepoStatusLoading(false);
    }
  }, [mainProjectPath]);

  useEffect(() => {
    void refreshMainRepoStatus();
  }, [refreshMainRepoStatus, targetBranch]);

  useEffect(() => {
    if (mergeDialogOpen) void refreshMainRepoStatus();
  }, [mergeDialogOpen, refreshMainRepoStatus]);

  const mainRepoDirty = mainRepoStatus?.isDirty === true;
  const patchActionsBlocked = mainRepoDirty;
  const mergeUiBlocked =
    !canMergeComb ||
    mainRepoDirty ||
    mainRepoStatusLoading;

  const handleMergeClick = () => {
    if (!mainProjectPath?.trim()) {
      toast.error("Caminho do repositório principal indisponível.");
      return;
    }
    if (!canMergeComb) {
      toast.error("Merge do branch só está disponível no target principal da Missão.");
      return;
    }
    void refreshMainRepoStatus().then(() => setMergeDialogOpen(true));
  };

  const handleConfirmMerge = async () => {
    const b = targetBranch.trim();
    if (!b) {
      toast.error("Selecione a branch de destino.");
      return;
    }
    if (!combId) return;
    setIsMerging(true);
    try {
      const result = await window.electronAPI?.comb?.mergeIntoMain(combId, b);
      if (result?.success) {
        toast.success("Missão mergeada com sucesso");
        setMergeDialogOpen(false);
        addTrail(`Merge na branch ${b} concluído`);
        onAction();
        await refreshMainRepoStatus();
      } else {
        toast.error(result?.error ?? "Erro ao fazer merge");
      }
    } finally {
      setIsMerging(false);
    }
  };

  const handleCommitWorktree = async (message: string) => {
    if (!worktreePath || !window.electronAPI?.git) return;
    const paths = includedPaths.length > 0 ? includedPaths : undefined;
    try {
      const ok = await window.electronAPI.git.commit(
        worktreePath,
        message,
        paths,
      );
      if (ok) {
        toast.success("Commit realizado no repositório");
        addTrail(
          `Commit: ${message.slice(0, 60)}${message.length > 60 ? "…" : ""}`,
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
    if (!worktreePath || !window.electronAPI?.git?.push) return;
    setIsPushing(true);
    try {
      const result = await window.electronAPI.git.push(worktreePath);
      if (result?.success) {
        toast.success("Push enviado");
        addTrail("Push do branch enviado");
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
    setIsApplyingPatch(true);
    try {
      if (useCombWorktreeApis && combId && window.electronAPI?.comb?.applyPatch) {
        const result = await window.electronAPI.comb.applyPatch(combId, b, {
          includeFiles: includedPaths,
          commit: opts.commit,
          message:
            opts.message?.trim() || `Apply from mission: ${missionName}`,
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
          await refreshMainRepoStatus();
          return true;
        }
        toast.error(result?.error ?? "Falha ao aplicar patch");
        return false;
      }

      const applyApi = window.electronAPI?.git?.applyWorktreePatch;
      if (!applyApi) {
        toast.error("Aplicar patch indisponível.");
        return false;
      }
      const result = await applyApi(
        mainProjectPath,
        worktreePath,
        b,
        {
          includeFiles: includedPaths,
          commit: opts.commit,
          message:
            opts.message?.trim() || `Apply from mission: ${missionName}`,
        },
      );
      if (result?.success) {
        toast.success(
          opts.commit
            ? "Patch aplicado e commitado no repositório principal"
            : "Patch aplicado no repositório principal",
        );
        addTrail(
          opts.commit
            ? `Patch aplicado com commit em ${b}`
            : `Patch aplicado em ${b}`,
        );
        onAction();
        await loadDiffs();
        await refreshMainRepoStatus();
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
    if (!worktreePath || !window.electronAPI?.git?.reset) return;
    const ok = await confirmDialog({
      title: "Descartar alterações locais?",
      description:
        "Será executado git reset --hard neste checkout/worktree. Isto não remove a Missão.",
      confirmLabel: "Descartar",
      cancelLabel: "Cancelar",
    });
    if (!ok) return;
    const result = await window.electronAPI.git.reset(worktreePath, "HEAD");
    if (result.success) {
      toast.success("Alterações locais descartadas");
      addTrail("Alterações locais descartadas (reset --hard)");
      await loadDiffs();
    } else {
      toast.error(result.error ?? "Falha ao descartar");
    }
  };

  useEffect(() => {
    if (!commitDialogOpen || !worktreePath || !window.electronAPI?.git) {
      if (!commitDialogOpen) setCommitDialogStatus(null);
      return;
    }
    window.electronAPI.git
      .getStatus(worktreePath)
      .then((s) => setCommitDialogStatus(s))
      .catch(() => setCommitDialogStatus(null));
  }, [commitDialogOpen, worktreePath]);

  useEffect(() => {
    if (applyCommitDialogOpen) {
      setApplyCommitMessage(`Apply from mission: ${missionName}`);
    }
  }, [applyCommitDialogOpen, missionName]);

  if (!worktreePath?.trim()) {
    return (
      <div className="rounded-lg border border-border p-4 text-sm text-muted-foreground">
        Target sem path Git: {title}
      </div>
    );
  }

  return (
    <div
      className={`flex min-h-0 flex-col rounded-lg border border-border bg-card/30 ${
        compact ? "" : "flex-1 min-h-0"
      }`}
    >
      <div className="shrink-0 space-y-3 border-b border-border px-4 py-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="space-y-1">
            <h4 className="text-sm font-semibold">{title}</h4>
            {subtitle ? (
              <p className="text-[11px] text-muted-foreground font-mono truncate max-w-[min(100%,480px)]">
                {subtitle}
              </p>
            ) : null}
            {diffs.summary && diffs.files.length > 0 && (
              <p className="text-xs text-muted-foreground">
                {diffs.files.length} arquivo(s): {summaryCounts.ok} OK,{" "}
                {summaryCounts.later} rever depois, {summaryCounts.suspicious}{" "}
                suspeito(s) · +{diffs.summary.insertions} −
                {diffs.summary.deletions}
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

        <Collapsible className="rounded-md border border-border/80 bg-muted/15">
          <CollapsibleTrigger className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-xs font-medium hover:bg-muted/30">
            <span>Ordem e estratégias de integração</span>
            <ChevronDown className="h-4 w-4 shrink-0 opacity-70" />
          </CollapsibleTrigger>
          <CollapsibleContent className="space-y-2 border-t border-border/60 px-3 pb-3 pt-0 text-xs text-muted-foreground">
            <ol className="mt-2 list-decimal space-y-1 pl-4 leading-relaxed">
              <li>Rever diffs e marcar ficheiros (para patch).</li>
              <li>Escolher a branch de destino no repositório principal.</li>
              <li>
                Opcional no branch da Missão: Commit local → Push (backup/remoto).
              </li>
              <li>
                No principal, escolher <strong className="text-foreground">uma</strong>{" "}
                estratégia:
              </li>
            </ol>
            <ul className="space-y-1.5 border-l-2 border-border pl-3">
              <li>
                <strong className="text-foreground">Merge</strong> — integra o branch
                inteiro (histórico). Exige o repositório principal{" "}
                <strong className="text-foreground">limpo</strong>.
              </li>
              <li>
                <strong className="text-foreground">Patch</strong> — copia alterações
                para a branch; &quot;Aplicar&quot; sem commit deixa o principal sujo até
                commitares ou descartares.
              </li>
            </ul>
            <p className="text-[11px] italic text-amber-600/90 dark:text-amber-400/90">
              Merge não é o passo natural depois de &quot;Aplicar&quot; sem commit —
              primeiro resolve o estado do repositório principal.
            </p>
          </CollapsibleContent>
        </Collapsible>

        {mainRepoStatusLoading ? (
          <p className="flex items-center gap-2 text-[11px] text-muted-foreground">
            <Loader2 className="h-3 w-3 animate-spin" /> A ler estado do
            repositório principal…
          </p>
        ) : null}

        {mainRepoDirty ? (
          <Alert variant="destructive" className="py-2">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle className="text-xs">Repositório principal com alterações pendentes</AlertTitle>
            <AlertDescription className="text-xs leading-snug">
              Patch e merge no principal estão bloqueados até commitares, stash ou
              descartares nesse repositório (não na worktree). Isto evita o erro de
              merge por ficheiros locais.
            </AlertDescription>
          </Alert>
        ) : null}

        <div className="space-y-2">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
            No branch da Missão (worktree)
          </p>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={diffs.files.length === 0}
              onClick={() => void handleDiscardWorktree()}
            >
              Descartar
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={diffs.files.length === 0}
              onClick={() => setCommitDialogOpen(true)}
            >
              <FileCode className="mr-1 h-3 w-3" />
              Commit…
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={isPushing || !worktreePath}
              onClick={() => void handlePush()}
            >
              {isPushing ? (
                <Loader2 className="mr-1 h-3 w-3 animate-spin" />
              ) : (
                <Upload className="mr-1 h-3 w-3" />
              )}
              Push
            </Button>
          </div>
        </div>

        <div className="space-y-2">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
            No repositório principal
          </p>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              disabled={
                isApplyingPatch ||
                !hasIncludedFiles ||
                diffs.files.length === 0 ||
                patchActionsBlocked
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
                isApplyingPatch ||
                !hasIncludedFiles ||
                diffs.files.length === 0 ||
                patchActionsBlocked
              }
              onClick={() => setApplyCommitDialogOpen(true)}
            >
              Aplicar + Commit
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={isMerging || mergeUiBlocked}
              title={
                !canMergeComb
                  ? "Disponível só no target principal da Missão."
                  : mainRepoStatusLoading
                    ? "A atualizar estado do repositório…"
                    : mainRepoDirty
                      ? "Limpe alterações no repositório principal antes do merge."
                      : undefined
              }
              onClick={handleMergeClick}
            >
              {isMerging ? (
                <Loader2 className="mr-1 h-3 w-3 animate-spin" />
              ) : (
                <GitMerge className="mr-1 h-3 w-3" />
              )}
              Integrar branch (merge)…
            </Button>
          </div>
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
          {mainRepoDirty ? (
            <Alert variant="destructive" className="py-2">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle className="text-xs">Principal sujo</AlertTitle>
              <AlertDescription className="text-xs">
                Não é possível fazer merge enquanto o repositório principal tiver
                alterações locais. Commit, stash ou descarte primeiro.
              </AlertDescription>
            </Alert>
          ) : null}
          <DialogFooter>
            <Button variant="outline" onClick={() => setMergeDialogOpen(false)}>
              Cancelar
            </Button>
            <Button
              onClick={() => void handleConfirmMerge()}
              disabled={isMerging || mergeUiBlocked}
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
            <Label htmlFor={`apply-commit-msg-${storageKey}`}>Mensagem</Label>
            <Textarea
              id={`apply-commit-msg-${storageKey}`}
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

      <ScrollArea
        className={
          compact ? "max-h-[min(70vh,560px)]" : "min-h-0 flex-1"
        }
      >
        {diffs.loading ? (
          <div className="flex items-center justify-center p-8">
            <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : diffs.error ? (
          <div className="p-4 text-sm text-destructive">{diffs.error}</div>
        ) : diffs.files.length === 0 ? (
          <div className="p-8 text-center text-sm text-muted-foreground">
            Nenhuma alteração detectada neste repositório.
          </div>
        ) : (
          <div className="space-y-4 p-4">
            {diffs.files.map((file) => {
              const tokens = extractContextTokens(file.diff);
              return (
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
                  {tokens.length > 0 ? (
                    <div className="flex flex-wrap gap-1 border-b border-border/60 bg-muted/20 px-3 py-1.5">
                      <span className="text-[10px] text-muted-foreground w-full">
                        Contexto
                      </span>
                      {tokens.map((tok) => (
                        <Badge
                          key={tok}
                          variant="secondary"
                          className="text-[10px] font-mono max-w-[200px] truncate"
                          title={tok}
                        >
                          {tok}
                        </Badge>
                      ))}
                    </div>
                  ) : null}
                  <DiffCodeBlock content={file.diff} />
                </div>
              );
            })}
          </div>
        )}
      </ScrollArea>

      <div
        className={`shrink-0 border-t border-border bg-muted/20 ${compact ? "py-1.5" : ""}`}
      >
        <div className="px-3 py-2">
          <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
            Trilha — {title}
          </p>
          <ScrollArea className={compact ? "h-16 pr-2" : "h-24 pr-2"}>
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
        defaultMessage={`Changes from mission: ${missionName}`}
        projectPath={worktreePath}
        status={commitDialogStatus}
      />
    </div>
  );
}
