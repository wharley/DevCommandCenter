"use client";

import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { cn } from "@/lib/utils";
import {
  AlertTriangle,
  ArrowDownToLine,
  Check,
  Clock,
  FileCode,
  GitMerge,
  Loader2,
  Maximize2,
  Minimize2,
  PanelRight,
  PanelRightClose,
  RefreshCw,
  Upload,
  X,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
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
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { DiffFileTree } from "@/components/review/diff-file-tree";
import { WorktreeNotesPanel } from "@/components/review/worktree-notes-panel";
import { PrReviewCommentsPanel } from "@/components/review/pr-review-comments-panel";
import { DiffCodeBlock } from "@/components/diff-code-block";
import { CommitDialog } from "@/components/dialogs/commit-dialog";
import { GitActionsPanel } from "@/components/review/git-actions-panel";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import { useIsMobile } from "@/hooks/use-mobile";
import { useMergePermissions } from "@/hooks/use-merge-permissions";
import { extractContextTokens } from "@/lib/review/extract-context-tokens";
import type { GitStatus } from "@/types/app";
import { toast } from "sonner";

export type ReviewFlag = "ok" | "later" | "suspicious";

type TrailEntry = { id: string; at: number; message: string };

const reviewFlagsKey = (storageKey: string) =>
  `dcc-review-flags-${storageKey}`;
const reviewIncludedKey = (storageKey: string) =>
  `dcc-review-included-${storageKey}`;
const reviewTrailKey = (storageKey: string) =>
  `dcc-review-trail-${storageKey}`;

const MAX_DIFF_TABS = 16;

function diffTabLabel(path: string): string {
  const parts = path.split(/[/\\]/);
  const base = parts[parts.length - 1] || path;
  return base.length > 28 ? `${base.slice(0, 25)}…` : base;
}

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
    files: Array<{
      path: string;
      status: string;
      diff: string;
      insertions?: number;
      deletions?: number;
    }>;
    summary: {
      changedFiles: number;
      insertions: number;
      deletions: number;
    } | null;
  }>({ loading: false, files: [], summary: null });

  /** Separadores de diff por path (ordem = ordem dos separadores). */
  const [diffOpenTabs, setDiffOpenTabs] = useState<string[]>([]);
  const [activeDiffTabPath, setActiveDiffTabPath] = useState<string | null>(
    null,
  );

  const [fileFlags, setFileFlags] = useState<Record<string, ReviewFlag>>({});
  const [included, setIncluded] = useState<Record<string, boolean>>({});
  const [trail, setTrail] = useState<TrailEntry[]>([]);

  const [targetBranch, setTargetBranch] = useState("");
  const [branchList, setBranchList] = useState<string[]>([]);

  const [isApplyingPatch, setIsApplyingPatch] = useState(false);
  const [isMerging, setIsMerging] = useState(false);
  const [isPushing, setIsPushing] = useState(false);
  const [isPulling, setIsPulling] = useState(false);

  const [commitDialogOpen, setCommitDialogOpen] = useState(false);
  const [commitDialogStatus, setCommitDialogStatus] =
    useState<GitStatus | null>(null);

  const [mergeDialogOpen, setMergeDialogOpen] = useState(false);
  const [applyCommitDialogOpen, setApplyCommitDialogOpen] = useState(false);
  const [applyCommitMessage, setApplyCommitMessage] = useState("");
  const [flowHelpOpen, setFlowHelpOpen] = useState(false);

  const [mainRepoStatus, setMainRepoStatus] = useState<GitStatus | null>(null);
  const [mainRepoStatusLoading, setMainRepoStatusLoading] = useState(false);

  /** Estado do Git na worktree (commit/merge só fazem sentido com cópia limpa antes do merge). */
  const [worktreeRepoStatus, setWorktreeRepoStatus] =
    useState<GitStatus | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [rightPanelCollapsed, setRightPanelCollapsed] = useState(false);
  const isMobile = useIsMobile();
  const sidebarPanelRef = useRef<React.ElementRef<typeof ResizablePanel> | null>(
    null,
  );
  const rightPanelRef = useRef<React.ElementRef<typeof ResizablePanel> | null>(null);

  const filePathsKey = useMemo(
    () => diffs.files.map((f) => f.path).sort().join("\0"),
    [diffs.files],
  );

  useEffect(() => {
    const paths = filePathsKey ? filePathsKey.split("\0") : [];
    const pathSet = new Set(paths);
    setDiffOpenTabs((prevTabs) => {
      const filtered = prevTabs.filter((p) => pathSet.has(p));
      const nextTabs =
        filtered.length === 0 && paths.length > 0 ? [paths[0]!] : filtered;
      setActiveDiffTabPath((active) => {
        if (nextTabs.length === 0) return null;
        if (active && nextTabs.includes(active)) return active;
        return nextTabs[0] ?? null;
      });
      return nextTabs;
    });
  }, [filePathsKey]);

  const navigateToDiffFile = useCallback((path: string) => {
    setDiffOpenTabs((prev) => {
      if (prev.includes(path)) return prev;
      const next = [...prev, path];
      return next.slice(-MAX_DIFF_TABS);
    });
    setActiveDiffTabPath(path);
  }, []);

  const closeDiffTab = useCallback(
    (path: string, e?: React.MouseEvent) => {
      e?.stopPropagation();
      e?.preventDefault();
      setDiffOpenTabs((prev) => {
        const idx = prev.indexOf(path);
        const next = prev.filter((p) => p !== path);
        setActiveDiffTabPath((active) => {
          if (active !== path) return active;
          if (next.length === 0) return null;
          if (idx <= 0) return next[0] ?? null;
          return next[Math.min(idx - 1, next.length - 1)] ?? next[0] ?? null;
        });
        return next;
      });
    },
    [],
  );

  const activeDiffFile = useMemo(
    () => diffs.files.find((f) => f.path === activeDiffTabPath),
    [diffs.files, activeDiffTabPath],
  );

  const activeDiffTokens = useMemo(
    () =>
      activeDiffFile ? extractContextTokens(activeDiffFile.diff) : [],
    [activeDiffFile],
  );

  const toggleReviewSidebar = useCallback(() => {
    const panel = sidebarPanelRef.current;
    if (!panel) return;
    if (panel.isCollapsed()) {
      panel.expand();
      return;
    }
    panel.collapse();
  }, []);

  const toggleRightPanel = useCallback(() => {
    const panel = rightPanelRef.current;
    if (!panel) return;
    if (panel.isCollapsed()) {
      panel.expand();
      return;
    }
    panel.collapse();
  }, []);

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

  const refreshWorktreeRepoStatus = useCallback(async () => {
    if (!worktreePath?.trim() || !window.desktopAPI?.git?.getStatus) {
      setWorktreeRepoStatus(null);
      return;
    }
    try {
      const s = await window.desktopAPI.git.getStatus(worktreePath);
      setWorktreeRepoStatus(s);
    } catch {
      setWorktreeRepoStatus(null);
    }
  }, [worktreePath]);

  const loadDiffs = useCallback(async () => {
    if (!worktreePath?.trim() || !window.desktopAPI?.git?.getReviewDiffs) return;
    setDiffs((prev) => ({ ...prev, loading: true, error: undefined }));
    const result = await window.desktopAPI.git.getReviewDiffs(worktreePath);
    if (result.success) {
      setDiffs({
        loading: false,
        files: result.files,
        summary: result.summary,
      });
      addTrail(
        `Alterações carregadas — ${title}: ${result.files.length} arquivo(s)`,
      );
      void refreshWorktreeRepoStatus();
    } else {
      setDiffs({
        loading: false,
        error: result.error ?? "Erro ao carregar diffs",
        files: [],
        summary: null,
      });
    }
  }, [worktreePath, title, addTrail, refreshWorktreeRepoStatus]);

  useEffect(() => {
    void loadDiffs();
  }, [loadDiffs]);

  useEffect(() => {
    void refreshWorktreeRepoStatus();
  }, [refreshWorktreeRepoStatus]);

  useEffect(() => {
    const t = loadReviewJson<TrailEntry[]>(reviewTrailKey(storageKey));
    setTrail(Array.isArray(t) ? t.slice(-20) : []);
  }, [storageKey]);

  useEffect(() => {
    if (!mainProjectPath?.trim()) return;
    const git = window.desktopAPI?.git;
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
    useCombWorktreeApis && combId && window.desktopAPI?.comb?.mergeIntoMain,
  );

  // Verificar permissões de merge/push na branch de destino
  const mergePermissions = useMergePermissions(
    useCombWorktreeApis && combId ? combId : null,
    targetBranch,
    {
      enabled: canMergeComb, // Só verificar se o merge está habilitado
    }
  );

  const refreshMainRepoStatus = useCallback(async () => {
    if (!mainProjectPath?.trim() || !window.desktopAPI?.git?.getStatus) {
      setMainRepoStatus(null);
      return;
    }
    setMainRepoStatusLoading(true);
    try {
      const s = await window.desktopAPI.git.getStatus(mainProjectPath);
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
    if (mergeDialogOpen) {
      void refreshMainRepoStatus();
      void refreshWorktreeRepoStatus();
    }
  }, [mergeDialogOpen, refreshMainRepoStatus, refreshWorktreeRepoStatus]);

  const mainRepoDirty = mainRepoStatus?.isDirty === true;
  const worktreeDirty = worktreeRepoStatus?.isDirty === true;
  const patchActionsBlocked = mainRepoDirty;
  /** Merge revalida a worktree em `handleConfirmMerge`; não bloquear por loading do primeiro getStatus. */
  const mergeUiBlocked =
    !canMergeComb ||
    mainRepoDirty ||
    worktreeDirty ||
    !mergePermissions.canMerge;

  const mainDirtyFilePreview = useMemo(() => {
    if (!mainRepoStatus?.isDirty) return [];
    const u = new Set<string>();
    for (const p of mainRepoStatus.staged ?? []) u.add(p);
    for (const p of mainRepoStatus.unstaged ?? []) u.add(p);
    for (const p of mainRepoStatus.untracked ?? []) u.add(p);
    return Array.from(u);
  }, [mainRepoStatus]);

  const handleMergeClick = () => {
    if (!mainProjectPath?.trim()) {
      toast.error("Caminho do repositório principal indisponível.");
      return;
    }
    if (!canMergeComb) {
      toast.error("Merge do branch só está disponível no target principal da Missão.");
      return;
    }
    void Promise.all([
      refreshMainRepoStatus(),
      refreshWorktreeRepoStatus(),
    ]).then(() => setMergeDialogOpen(true));
  };

  const handleConfirmMerge = async () => {
    const b = targetBranch.trim();
    if (!b) {
      toast.error("Selecione a branch de destino.");
      return;
    }
    if (!combId) return;
    if (worktreePath && window.desktopAPI?.git?.getStatus) {
      try {
        const wt = await window.desktopAPI.git.getStatus(worktreePath);
        if (wt.isDirty) {
          toast.error(
            "A worktree ainda tem alterações por commitar. Faça commit ou descarte antes do merge.",
          );
          return;
        }
      } catch {
        toast.error("Não foi possível verificar o estado da worktree.");
        return;
      }
    }
    setIsMerging(true);
    try {
      const result = await window.desktopAPI?.comb?.mergeIntoMain(combId, b);
      if (result?.success) {
        toast.success("Missão mergeada com sucesso");
        setMergeDialogOpen(false);
        addTrail(`Merge na branch ${b} concluído`);
        onAction();
        await refreshMainRepoStatus();
        await refreshWorktreeRepoStatus();
      } else {
        toast.error(result?.error ?? "Erro ao fazer merge");
      }
    } finally {
      setIsMerging(false);
    }
  };

  const handleCommitWorktree = async (message: string) => {
    if (!worktreePath || !window.desktopAPI?.git) return;
    try {
      const result = await window.desktopAPI.git.commit(
        worktreePath,
        message,
      );
      if (result.success) {
        toast.success("Commit realizado no repositório");
        addTrail(
          `Commit: ${message.slice(0, 60)}${message.length > 60 ? "…" : ""}`,
        );
        await loadDiffs();
        await refreshWorktreeRepoStatus();
        await refreshMainRepoStatus();
      } else {
        toast.error(
          result.error ??
            "Commit não realizado — costuma acontecer quando não há alterações para incluir ou o Git recusou o commit.",
        );
        throw new Error(result.error ?? "Commit não realizado");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro desconhecido";
      toast.error(msg);
      throw e;
    }
  };

  const handlePush = async () => {
    if (!worktreePath || !window.desktopAPI?.git?.push) return;
    setIsPushing(true);
    try {
      const result = await window.desktopAPI.git.push(worktreePath);
      if (result?.success) {
        toast.success("Push enviado");
        addTrail("Push do branch enviado");
        await loadDiffs();
        await refreshWorktreeRepoStatus();
      } else {
        toast.error(result?.error ?? "Falha ao enviar push");
      }
    } finally {
      setIsPushing(false);
    }
  };

  const handlePull = async () => {
    if (!worktreePath || !window.desktopAPI?.git?.pull) return;
    setIsPulling(true);
    try {
      const result = await window.desktopAPI.git.pull(worktreePath);
      if (result?.success) {
        toast.success("Pull concluído na worktree");
        addTrail("Pull na worktree");
        await loadDiffs();
        await refreshWorktreeRepoStatus();
      } else {
        toast.error(result?.error ?? "Falha ao fazer pull");
      }
    } finally {
      setIsPulling(false);
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
      if (useCombWorktreeApis && combId && window.desktopAPI?.comb?.applyPatch) {
        const result = await window.desktopAPI.comb.applyPatch(combId, b, {
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

      const applyApi = window.desktopAPI?.git?.applyWorktreePatch;
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
    if (!worktreePath || !window.desktopAPI?.git?.reset) return;
    const ok = await confirmDialog({
      title: "Descartar alterações locais?",
      description:
        "Será executado git reset --hard neste checkout/worktree. Isto não remove a Missão.",
      confirmLabel: "Descartar",
      cancelLabel: "Cancelar",
    });
    if (!ok) return;
    const result = await window.desktopAPI.git.reset(worktreePath, "HEAD");
    if (result.success) {
      toast.success("Alterações locais descartadas");
      addTrail("Alterações locais descartadas (reset --hard)");
      await loadDiffs();
      await refreshWorktreeRepoStatus();
    } else {
      toast.error(result.error ?? "Falha ao descartar");
    }
  };

  const handleDiscardMainRepo = async () => {
    if (!mainProjectPath?.trim() || !window.desktopAPI?.git?.reset) return;
    const ok = await confirmDialog({
      title: "Descartar alterações no repositório principal?",
      description:
        "Será executado git reset --hard HEAD no checkout principal (não na worktree da Missão). Perdes alterações locais não commitadas nesse repositório.",
      confirmLabel: "Descartar no principal",
      cancelLabel: "Cancelar",
    });
    if (!ok) return;
    const result = await window.desktopAPI.git.reset(mainProjectPath, "HEAD");
    if (result.success) {
      toast.success("Alterações locais descartadas no repositório principal");
      addTrail("Principal: reset --hard (working tree limpo)");
      await refreshMainRepoStatus();
    } else {
      toast.error(result.error ?? "Falha ao descartar no principal");
    }
  };

  useEffect(() => {
    if (!commitDialogOpen || !worktreePath || !window.desktopAPI?.git) {
      if (!commitDialogOpen) setCommitDialogStatus(null);
      return;
    }
    window.desktopAPI.git
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
      <div className="shrink-0 border-b border-border px-4 py-3 space-y-2.5">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0 flex-1 space-y-0.5">
            <h4 className="text-sm font-semibold">{title}</h4>
            {subtitle ? (
              <p className="text-[11px] text-muted-foreground font-mono truncate max-w-[min(100%,480px)]">
                {subtitle}
              </p>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-1">
            {diffs.summary && diffs.files.length > 0 && (
              <div className="hidden sm:flex items-center gap-1 font-mono text-[11px] mr-1">
                <span className="text-emerald-600 dark:text-emerald-400">
                  +{diffs.summary.insertions}
                </span>
                <span className="text-muted-foreground opacity-40">/</span>
                <span className="text-rose-600 dark:text-rose-400">
                  −{diffs.summary.deletions}
                </span>
              </div>
            )}
            {mainRepoStatusLoading && (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
            )}
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={() => void loadDiffs()}
              title="Recarregar diffs"
              disabled={diffs.loading}
            >
              {diffs.loading ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5" />
              )}
            </Button>
            {!isMobile && (
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={toggleRightPanel}
                title={
                  rightPanelCollapsed
                    ? "Mostrar painel de ações"
                    : "Ocultar painel de ações"
                }
              >
                {rightPanelCollapsed ? (
                  <PanelRight className="h-3.5 w-3.5" />
                ) : (
                  <PanelRightClose className="h-3.5 w-3.5" />
                )}
              </Button>
            )}
          </div>
        </div>

        {diffs.summary && diffs.files.length > 0 && (
          <p className="text-xs text-muted-foreground">
            {diffs.files.length} arquivo(s)
            {" · "}
            <span className="text-emerald-600 dark:text-emerald-400">
              {summaryCounts.ok} ok
            </span>
            {summaryCounts.later > 0 && (
              <>
                {" · "}
                <span className="text-amber-500">
                  {summaryCounts.later} rever depois
                </span>
              </>
            )}
            {summaryCounts.suspicious > 0 && (
              <>
                {" · "}
                <span className="text-rose-500">
                  {summaryCounts.suspicious} suspeito(s)
                </span>
              </>
            )}
          </p>
        )}
        {diffs.summary && diffs.files.length === 0 && !diffs.loading && (
          <p className="text-xs text-muted-foreground">Nenhuma alteração pendente</p>
        )}

        {mainRepoDirty ? (
          <Alert variant="destructive" className="py-2">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle className="text-xs">Repositório principal com alterações pendentes</AlertTitle>
            <AlertDescription className="space-y-2 text-xs leading-snug">
              <p>
                Patch e merge bloqueados até commitares, stash ou descartares nesse
                repositório (não na worktree).
              </p>
              {mainDirtyFilePreview.length > 0 ? (
                <span className="block font-mono text-[10px] text-destructive/95">
                  {mainDirtyFilePreview.slice(0, 8).join(" · ")}
                  {mainDirtyFilePreview.length > 8
                    ? ` · +${mainDirtyFilePreview.length - 8} mais`
                    : ""}
                </span>
              ) : null}
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="h-7 border-destructive/40 text-xs"
                onClick={() => void handleDiscardMainRepo()}
              >
                Descartar alterações locais no principal…
              </Button>
            </AlertDescription>
          </Alert>
        ) : null}

        {canMergeComb && worktreeDirty ? (
          <Alert variant="destructive" className="py-2">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle className="text-xs">Worktree com alterações por commitar</AlertTitle>
            <AlertDescription className="text-xs leading-snug">
              O merge integra o branch já commitado. Faz commit ou descarta na worktree
              antes de integrar no repositório principal.
            </AlertDescription>
          </Alert>
        ) : null}

        {isMobile && (
          <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1 text-xs"
              disabled={diffs.files.length === 0}
              onClick={() => setCommitDialogOpen(true)}
            >
              <FileCode className="h-3 w-3" />
              Commit…
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1 text-xs"
              disabled={isPushing || isPulling || isMerging}
              onClick={() => void handlePush()}
            >
              {isPushing ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <Upload className="h-3 w-3" />
              )}
              Push
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1 text-xs"
              disabled={isPushing || isPulling || isMerging}
              onClick={() => void handlePull()}
            >
              {isPulling ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <ArrowDownToLine className="h-3 w-3" />
              )}
              Pull
            </Button>
            {canMergeComb && (
              <Button
                size="sm"
                variant="default"
                className="h-7 gap-1 text-xs"
                disabled={mergeUiBlocked}
                onClick={handleMergeClick}
              >
                {isMerging ? (
                  <Loader2 className="h-3 w-3 animate-spin" />
                ) : (
                  <GitMerge className="h-3 w-3" />
                )}
                Merge…
              </Button>
            )}
          </div>
        )}
      </div>

      {/* ── DIALOGS ────────────────────────────────────────────── */}
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
          {worktreeDirty ? (
            <Alert variant="destructive" className="py-2">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle className="text-xs">Worktree suja</AlertTitle>
              <AlertDescription className="text-xs">
                Faça commit ou descarte na worktree antes do merge — só entram no merge
                os commits já gravados no branch.
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

      <Dialog open={flowHelpOpen} onOpenChange={setFlowHelpOpen}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>Fluxo de review e integração</DialogTitle>
            <DialogDescription>
              Este resumo detalha o caminho recomendado para usar a worktree como
              área de decisão e manter o diff como foco principal.
            </DialogDescription>
          </DialogHeader>
          <ScrollArea className="max-h-[60vh] pr-3">
            <div className="space-y-4 py-2 text-sm">
              <section className="rounded-md border border-border/70 bg-muted/20 p-3">
                <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Passo 1
                </p>
                <p className="mt-1 font-medium text-foreground">Worktree da Missão</p>
                <p className="mt-1 leading-relaxed text-muted-foreground">
                  Faz commit e, se necessário, push do branch da Missão. Usa pull
                  apenas para alinhar com o remoto antes de integrar.
                </p>
              </section>
              <section className="rounded-md border border-border/70 bg-muted/20 p-3">
                <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Passo 2
                </p>
                <p className="mt-1 font-medium text-foreground">Repositório principal limpo</p>
                <p className="mt-1 leading-relaxed text-muted-foreground">
                  O checkout principal precisa estar limpo. Se houver alterações
                  locais, faz commit, stash ou descarta no principal antes do merge.
                </p>
              </section>
              <section className="rounded-md border border-border/70 bg-muted/20 p-3">
                <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Integração
                </p>
                <p className="mt-1 leading-relaxed text-muted-foreground">
                  O merge traz o branch inteiro da Missão para a branch de destino.
                </p>
              </section>
              <section className="rounded-md border border-border/70 bg-muted/20 p-3">
                <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Pasta da Missão
                </p>
                <p className="mt-1 break-all font-mono text-[11px] text-foreground">
                  {worktreePath}
                </p>
              </section>
            </div>
          </ScrollArea>
          <DialogFooter>
            <Button variant="outline" onClick={() => setFlowHelpOpen(false)}>
              Fechar
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* ── PANELS ─────────────────────────────────────────────── */}
      {isMobile ? (
        /* Mobile: vertical 2-panel (unchanged behavior) */
        <ResizablePanelGroup
          direction="vertical"
          className={cn(
            "min-h-0 flex-1 px-4 pb-4",
            compact ? "max-h-[min(70vh,560px)]" : "",
          )}
        >
          <ResizablePanel
            ref={sidebarPanelRef}
            defaultSize={36}
            minSize={20}
            maxSize={48}
            collapsible
            collapsedSize={0}
            onCollapse={() => setSidebarCollapsed(true)}
            onExpand={() => setSidebarCollapsed(false)}
            className="min-h-0 overflow-hidden"
          >
            <div className="flex h-full min-h-0 flex-col gap-3 pb-3">
              <WorktreeNotesPanel
                worktreePath={worktreePath}
                idKey={storageKey}
                compact={false}
              />
              <PrReviewCommentsPanel
                combId={combId}
                enabled={Boolean(useCombWorktreeApis && combId)}
                activeFilePath={activeDiffFile?.path ?? null}
                idKey={storageKey}
                compact={false}
              />
              <div className="shrink-0 rounded-md border border-border bg-muted/20">
                <div className="px-3 py-2">
                  <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                    Trilha — {title}
                  </p>
                  <ScrollArea className="h-16 pr-2">
                    <ul className="space-y-1.5 text-xs text-muted-foreground">
                      {trail.length === 0 ? (
                        <li className="italic opacity-70">Eventos de revisão aparecem aqui.</li>
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
            </div>
          </ResizablePanel>
          <ResizableHandle
            withHandle
            className="bg-border/70 my-2"
          />
          <ResizablePanel
            defaultSize={64}
            minSize={52}
            className="min-h-0 overflow-hidden"
          >
            <div
              className={cn(
                "flex min-h-0 flex-1 flex-col",
                compact ? "max-h-[min(70vh,560px)]" : "",
              )}
            >
              {diffs.loading ? null : diffs.error ? null : diffs.files.length === 0 ? null : (
                <DiffFileTree
                  files={diffs.files}
                  selectedPath={activeDiffTabPath}
                  onFileSelect={navigateToDiffFile}
                  compact={compact}
                />
              )}
              <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                {diffs.loading ? null : diffs.error ? null : diffs.files.length === 0 ? null : (
                  <div
                    className="flex shrink-0 gap-1 overflow-x-auto border-b border-border bg-muted/20 px-2 py-1.5"
                    role="tablist"
                    aria-label="Separadores de diff"
                  >
                    {diffOpenTabs.map((tabPath) => {
                      const active = tabPath === activeDiffTabPath;
                      return (
                        <div
                          key={tabPath}
                          role="tab"
                          aria-selected={active}
                          className={cn(
                            "group flex max-w-[200px] shrink-0 cursor-pointer items-center gap-0.5 rounded-md border px-2 py-1 text-left text-[11px] transition-colors",
                            active
                              ? "border-primary bg-primary/10 text-foreground"
                              : "border-border/80 bg-muted/40 text-muted-foreground hover:bg-muted/70",
                          )}
                          onClick={() => setActiveDiffTabPath(tabPath)}
                        >
                          <span className="min-w-0 flex-1 truncate font-mono" title={tabPath}>
                            {diffTabLabel(tabPath)}
                          </span>
                          <button
                            type="button"
                            className="shrink-0 rounded p-0.5 opacity-60 hover:bg-background/80 hover:opacity-100"
                            aria-label={`Fechar diff ${tabPath}`}
                            onClick={(e) => closeDiffTab(tabPath, e)}
                          >
                            <X className="h-3 w-3" />
                          </button>
                        </div>
                      );
                    })}
                  </div>
                )}
                <ScrollArea className="min-h-0 flex-1">
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
                  ) : !activeDiffFile ? (
                    <div className="p-6 text-center text-sm text-muted-foreground">
                      Seleciona um ficheiro na árvore ou num separador.
                    </div>
                  ) : (
                    <div className="space-y-4 p-4">
                      <div className="overflow-hidden rounded-xl border border-border shadow-sm ring-1 ring-primary/5">
                        <div className="sticky top-0 z-20 flex flex-wrap items-center gap-2 border-b border-border/80 bg-background/95 px-3 py-2 backdrop-blur">
                          <Checkbox
                            checked={included[activeDiffFile.path] !== false}
                            onCheckedChange={(c) =>
                              setIncluded((prev) => ({
                                ...prev,
                                [activeDiffFile.path]: c === true,
                              }))
                            }
                            aria-label={`Incluir ${activeDiffFile.path} no patch`}
                          />
                          <ToggleGroup
                            type="single"
                            value={fileFlags[activeDiffFile.path] ?? "ok"}
                            onValueChange={(v) => {
                              if (!v) return;
                              setFileFlags((prev) => ({
                                ...prev,
                                [activeDiffFile.path]: v as ReviewFlag,
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
                            {activeDiffFile.path}
                          </span>
                          {(activeDiffFile.insertions ?? 0) > 0 ||
                          (activeDiffFile.deletions ?? 0) > 0 ? (
                            <span
                              className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground"
                              title="Inserções / remoções (diff)"
                            >
                              <span className="text-emerald-600 dark:text-emerald-400">
                                +{activeDiffFile.insertions ?? 0}
                              </span>
                              <span className="mx-0.5 opacity-40">/</span>
                              <span className="text-rose-600 dark:text-rose-400">
                                −{activeDiffFile.deletions ?? 0}
                              </span>
                            </span>
                          ) : null}
                          <Badge variant="outline" className="shrink-0 text-[10px]">
                            {activeDiffFile.status}
                          </Badge>
                        </div>
                        {activeDiffTokens.length > 0 ? (
                          <div className="flex flex-wrap gap-1 border-b border-border/60 bg-muted/20 px-3 py-1.5">
                            <span className="text-[10px] text-muted-foreground w-full">
                              Contexto
                            </span>
                            {activeDiffTokens.map((tok) => (
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
                        <DiffCodeBlock
                          content={activeDiffFile.diff}
                          maxHeightClassName="max-h-none"
                          className="bg-background/40"
                        />
                      </div>
                    </div>
                  )}
                </ScrollArea>
              </div>
            </div>
          </ResizablePanel>
        </ResizablePanelGroup>
      ) : (
        /* Desktop: horizontal 3-panel */
        <ResizablePanelGroup
          direction="horizontal"
          className={cn(
            "min-h-0 flex-1 px-4 pb-4",
            compact ? "max-h-[min(70vh,560px)]" : "",
          )}
        >
          {/* Left: Notes / PR Comments / Trail */}
          <ResizablePanel
            ref={sidebarPanelRef}
            defaultSize={22}
            minSize={16}
            maxSize={32}
            collapsible
            collapsedSize={0}
            onCollapse={() => setSidebarCollapsed(true)}
            onExpand={() => setSidebarCollapsed(false)}
            className="min-h-0 overflow-hidden"
          >
            <div className="flex h-full min-h-0 flex-col gap-3 pr-3">
              <WorktreeNotesPanel
                worktreePath={worktreePath}
                idKey={storageKey}
                compact={false}
              />
              <PrReviewCommentsPanel
                combId={combId}
                enabled={Boolean(useCombWorktreeApis && combId)}
                activeFilePath={activeDiffFile?.path ?? null}
                idKey={storageKey}
                compact={false}
              />
              <div className="shrink-0 rounded-md border border-border bg-muted/20">
                <div className="px-3 py-2">
                  <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                    Trilha — {title}
                  </p>
                  <ScrollArea className="h-24 pr-2">
                    <ul className="space-y-1.5 text-xs text-muted-foreground">
                      {trail.length === 0 ? (
                        <li className="italic opacity-70">Eventos de revisão aparecem aqui.</li>
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
            </div>
          </ResizablePanel>

          <ResizableHandle withHandle className="bg-border/70 mx-2" />

          {/* Center: Diff viewer */}
          <ResizablePanel
            defaultSize={55}
            minSize={35}
            className="min-h-0 overflow-hidden"
          >
            <div className="flex h-full min-h-0 min-w-0 flex-col">
              {diffs.loading ? null : diffs.error ? null : diffs.files.length === 0 ? null : (
                <div
                  className="flex shrink-0 gap-1 overflow-x-auto border-b border-border bg-muted/20 px-2 py-1.5"
                  role="tablist"
                  aria-label="Separadores de diff"
                >
                  {diffOpenTabs.map((tabPath) => {
                    const active = tabPath === activeDiffTabPath;
                    return (
                      <div
                        key={tabPath}
                        role="tab"
                        aria-selected={active}
                        className={cn(
                          "group flex max-w-[200px] shrink-0 cursor-pointer items-center gap-0.5 rounded-md border px-2 py-1 text-left text-[11px] transition-colors",
                          active
                            ? "border-primary bg-primary/10 text-foreground"
                            : "border-border/80 bg-muted/40 text-muted-foreground hover:bg-muted/70",
                        )}
                        onClick={() => setActiveDiffTabPath(tabPath)}
                      >
                        <span
                          className="min-w-0 flex-1 truncate font-mono"
                          title={tabPath}
                        >
                          {diffTabLabel(tabPath)}
                        </span>
                        <button
                          type="button"
                          className="shrink-0 rounded p-0.5 opacity-60 hover:bg-background/80 hover:opacity-100"
                          aria-label={`Fechar diff ${tabPath}`}
                          onClick={(e) => closeDiffTab(tabPath, e)}
                        >
                          <X className="h-3 w-3" />
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}
              <ScrollArea className="min-h-0 flex-1">
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
                ) : !activeDiffFile ? (
                  <div className="p-6 text-center text-sm text-muted-foreground">
                    Seleciona um ficheiro na árvore ou num separador.
                  </div>
                ) : (
                  <div className="space-y-4 p-4">
                    <div className="overflow-hidden rounded-xl border border-border shadow-sm ring-1 ring-primary/5">
                      <div className="sticky top-0 z-20 flex flex-wrap items-center gap-2 border-b border-border/80 bg-background/95 px-3 py-2 backdrop-blur">
                        <Checkbox
                          checked={included[activeDiffFile.path] !== false}
                          onCheckedChange={(c) =>
                            setIncluded((prev) => ({
                              ...prev,
                              [activeDiffFile.path]: c === true,
                            }))
                          }
                          aria-label={`Incluir ${activeDiffFile.path} no patch`}
                        />
                        <ToggleGroup
                          type="single"
                          value={fileFlags[activeDiffFile.path] ?? "ok"}
                          onValueChange={(v) => {
                            if (!v) return;
                            setFileFlags((prev) => ({
                              ...prev,
                              [activeDiffFile.path]: v as ReviewFlag,
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
                          {activeDiffFile.path}
                        </span>
                        {(activeDiffFile.insertions ?? 0) > 0 ||
                        (activeDiffFile.deletions ?? 0) > 0 ? (
                          <span
                            className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground"
                            title="Inserções / remoções (diff)"
                          >
                            <span className="text-emerald-600 dark:text-emerald-400">
                              +{activeDiffFile.insertions ?? 0}
                            </span>
                            <span className="mx-0.5 opacity-40">/</span>
                            <span className="text-rose-600 dark:text-rose-400">
                              −{activeDiffFile.deletions ?? 0}
                            </span>
                          </span>
                        ) : null}
                        <Badge variant="outline" className="shrink-0 text-[10px]">
                          {activeDiffFile.status}
                        </Badge>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          className="ml-auto gap-1 whitespace-nowrap"
                          onClick={toggleReviewSidebar}
                          title={
                            sidebarCollapsed
                              ? "Mostrar notas e comentários"
                              : "Focar no diff e recolher a lateral"
                          }
                        >
                          {sidebarCollapsed ? (
                            <Minimize2 className="h-3.5 w-3.5" />
                          ) : (
                            <Maximize2 className="h-3.5 w-3.5" />
                          )}
                          {sidebarCollapsed ? "Mostrar lateral" : "Focar diff"}
                        </Button>
                      </div>
                      {activeDiffTokens.length > 0 ? (
                        <div className="flex flex-wrap gap-1 border-b border-border/60 bg-muted/20 px-3 py-1.5">
                          <span className="text-[10px] text-muted-foreground w-full">
                            Contexto
                          </span>
                          {activeDiffTokens.map((tok) => (
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
                      <DiffCodeBlock
                        content={activeDiffFile.diff}
                        maxHeightClassName="max-h-none"
                        className="bg-background/40"
                      />
                    </div>
                  </div>
                )}
              </ScrollArea>
            </div>
          </ResizablePanel>

          <ResizableHandle withHandle className="bg-border/70 mx-2" />

          {/* Right: Git Actions Panel */}
          <ResizablePanel
            ref={rightPanelRef}
            defaultSize={23}
            minSize={18}
            maxSize={38}
            collapsible
            collapsedSize={0}
            onCollapse={() => setRightPanelCollapsed(true)}
            onExpand={() => setRightPanelCollapsed(false)}
            className="min-h-0 overflow-hidden"
          >
            <GitActionsPanel
              files={diffs.files}
              fileFlags={fileFlags}
              activeDiffPath={activeDiffTabPath}
              onFileSelect={navigateToDiffFile}
              isPushing={isPushing}
              isPulling={isPulling}
              isMerging={isMerging}
              worktreeDirty={worktreeDirty}
              mainRepoDirty={mainRepoDirty}
              canMergeComb={canMergeComb}
              mergeUiBlocked={mergeUiBlocked}
              onCommit={() => setCommitDialogOpen(true)}
              onPush={() => void handlePush()}
              onPull={() => void handlePull()}
              onDiscard={() => void handleDiscardWorktree()}
              onMerge={handleMergeClick}
              targetBranch={targetBranch}
              branchList={branchList}
              onTargetBranchChange={setTargetBranch}
            />
          </ResizablePanel>
        </ResizablePanelGroup>
      )}

      <CommitDialog
        open={commitDialogOpen}
        onOpenChange={setCommitDialogOpen}
        onCommit={handleCommitWorktree}
        defaultMessage={`Changes from mission: ${missionName}`}
        projectPath={worktreePath}
        status={
          commitDialogOpen
            ? (commitDialogStatus ?? worktreeRepoStatus)
            : null
        }
      />
    </div>
  );
}
