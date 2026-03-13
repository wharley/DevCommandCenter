"use client";

import React, { useEffect, useRef, useState, useMemo } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import {
  ArrowLeft,
  Loader2,
  Terminal,
  MessageSquare,
  Clock,
  GitBranch,
  GitCommit,
  GitMerge,
  Upload,
  Trash2,
  Copy,
  FileText,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useProjects, useMissions, useProviders, useMissionLogs } from "@/hooks/use-data";
import { EmbeddedTerminal } from "@/components/embedded-terminal";
import { DiffCodeBlock } from "@/components/diff-code-block";
import { CommitDialog } from "@/components/dialogs/commit-dialog";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import { toast } from "sonner";
import { format, formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import type { Mission, MissionLogType } from "@/lib/database/types";
import type { GitBranchState, GitStatus } from "@/types/electron";

const logTypeConfig: Record<
  MissionLogType,
  { icon: React.ElementType; color: string }
> = {
  info: { icon: MessageSquare, color: "text-muted-foreground" },
  prompt: { icon: MessageSquare, color: "text-blue-500" },
  response: { icon: MessageSquare, color: "text-primary" },
  error: { icon: MessageSquare, color: "text-destructive" },
  action: { icon: MessageSquare, color: "text-green-500" },
  user_input: { icon: MessageSquare, color: "text-amber-500" },
  warning: { icon: MessageSquare, color: "text-amber-500" },
  debug: { icon: MessageSquare, color: "text-muted-foreground" },
};

function getTaskStatusLabel(mission: Mission): "Em execução" | "Pronta para revisão" | "Histórico" | "Na fila" {
  if (mission.context?.agentSession?.status === "running") return "Em execução";
  if (mission.worktreePath && ["completed", "failed", "cancelled"].includes(mission.status)) {
    return "Pronta para revisão";
  }
  if (["completed", "failed", "cancelled"].includes(mission.status)) {
    return "Histórico";
  }
  return "Na fila";
}

function getTaskStatusVariant(status: ReturnType<typeof getTaskStatusLabel>) {
  if (status === "Em execução") return "default" as const;
  if (status === "Pronta para revisão") return "secondary" as const;
  if (status === "Histórico") return "outline" as const;
  return "outline" as const;
}

function getFriendlyBranchLabel(branch?: string | null): string | null {
  if (!branch || !branch.startsWith("dcc-mission-")) return null;
  const raw = branch.replace(/^dcc-mission-/, "");
  const withoutId = raw.replace(/-[a-z0-9]{8}$/i, "");
  if (!withoutId) return null;
  const words = withoutId
    .split("-")
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1));
  return words.length > 0 ? words.join(" ") : null;
}

export default function TaskPage() {
  const { id: projectId, missionId } = useParams<{ id: string; missionId: string }>();
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState("terminal");
  const [embeddedTerminalCwd, setEmbeddedTerminalCwd] = useState<string | null>(null);
  const [embeddedTerminalCommand, setEmbeddedTerminalCommand] = useState<string | undefined>(undefined);
  const [embeddedTerminalArgs, setEmbeddedTerminalArgs] = useState<string[]>([]);
  const [isOpeningTerminal, setIsOpeningTerminal] = useState(false);
  const [branchState, setBranchState] = useState<GitBranchState | null>(null);
  const [diffState, setDiffState] = useState<{
    loading: boolean;
    error?: string;
    files: Array<{ path: string; diff: string }>;
  }>({
    loading: false,
    files: [],
  });
  const hasAutoOpenedRef = useRef<string | null>(null);
  const [commitDialogOpen, setCommitDialogOpen] = useState(false);
  const [commitDialogStatus, setCommitDialogStatus] = useState<GitStatus | null>(
    null,
  );
  const [isPushing, setIsPushing] = useState(false);
  const [isWorktreeAction, setIsWorktreeAction] = useState(false);

  const { projects, isLoading: projectsLoading } = useProjects();
  const {
    missions,
    refresh: refreshMissions,
    update,
    isLoading: missionsLoading,
  } = useMissions(projectId ?? undefined);
  const { providers } = useProviders();
  const { logs } = useMissionLogs(missionId ?? "");
  const { confirmDialog } = useConfirmDialog();

  const project = useMemo(
    () => (projectId ? projects.find((p) => p.id === projectId) ?? null : null),
    [projectId, projects]
  );
  const mission = useMemo(
    () => (missionId ? missions.find((m) => m.id === missionId) ?? null : null),
    [missionId, missions]
  );

  const suggestedCliCommand = useMemo(() => {
    const prov = mission?.providerId
      ? providers.find((p) => p.id === mission.providerId)
      : null;
    if (!prov) return undefined;
    const t = prov.type;
    const cliPath = prov.cliPath?.trim();
    const usePath =
      cliPath && (cliPath.startsWith("/") || /^[A-Za-z]:\\/.test(cliPath));
    if (t === "codex") return usePath ? cliPath : "codex";
    if (t === "claude-code") return usePath ? cliPath : "claude";
    if (t === "gemini") return usePath ? cliPath : "gemini";
    if (t === "cursor") return usePath ? cliPath : "cursor-agent";
    return undefined;
  }, [providers, mission?.providerId]);

  const missionPrompt = useMemo(() => {
    if (!mission) return "";
    const parts = [mission.title, mission.description];
    if (mission.preserveInstructions?.trim()) {
      parts.push(`Não alterar: ${mission.preserveInstructions.trim()}`);
    }
    return parts.join("\n\n");
  }, [mission?.title, mission?.description, mission?.preserveInstructions]);

  const reviewPath = mission?.worktreePath ?? embeddedTerminalCwd ?? null;
  const taskStatusLabel = mission ? getTaskStatusLabel(mission) : "Na fila";
  const currentBranchLabel =
    branchState?.branch ??
    mission?.worktreeBranch ??
    (mission?.worktreePath ? "preparando..." : "branch não criada");
  const friendlyBranchLabel = getFriendlyBranchLabel(
    branchState?.branch ?? mission?.worktreeBranch,
  );
  const shouldHideLogsTab =
    mission?.missionType === "agents_cli" && logs.length === 0;

  const openTerminal = async () => {
    if (
      !missionId ||
      !project?.path ||
      !window.electronAPI?.worktree?.ensureForMission ||
      !window.electronAPI?.terminal?.getOrCreate
    )
      return;
    setIsOpeningTerminal(true);
    try {
      const ensure = await window.electronAPI?.worktree?.ensureForMission(missionId);
      if (!ensure || !ensure.success) {
        toast.error(ensure?.error ?? "Não foi possível criar/obter worktree");
        return;
      }
      const pathToOpen = ensure.worktreePath ?? project.path;
      setEmbeddedTerminalCwd(pathToOpen);
      setEmbeddedTerminalCommand(suggestedCliCommand ?? undefined);
      setEmbeddedTerminalArgs(missionPrompt ? [missionPrompt] : []);
      setActiveTab("terminal");
      if (ensure.worktreePath) refreshMissions();
    } catch (e) {
      toast.error(`Erro: ${e instanceof Error ? e.message : "desconhecido"}`);
    } finally {
      setIsOpeningTerminal(false);
    }
  };

  useEffect(() => {
    if (
      !missionId ||
      !project?.path ||
      !mission ||
      mission.missionType !== "agents_cli" ||
      hasAutoOpenedRef.current === missionId
    )
      return;
    if (!window.electronAPI?.worktree?.ensureForMission || !window.electronAPI?.terminal?.getOrCreate)
      return;
    hasAutoOpenedRef.current = missionId;
    let cancelled = false;
    (async () => {
      try {
        const ensure = await window.electronAPI?.worktree?.ensureForMission(missionId);
        if (cancelled) return;
        if (!ensure?.success) return;
        const pathToOpen = ensure.worktreePath ?? project.path;
        setEmbeddedTerminalCwd(pathToOpen);
        setEmbeddedTerminalCommand(suggestedCliCommand ?? undefined);
        setEmbeddedTerminalArgs(missionPrompt ? [missionPrompt] : []);
        setActiveTab("terminal");
        if (ensure.worktreePath) refreshMissions();
      } catch {
        if (!cancelled) hasAutoOpenedRef.current = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [missionId, project?.path, mission?.missionType, suggestedCliCommand, missionPrompt, refreshMissions]);

  useEffect(() => {
    if (!reviewPath || !window.electronAPI?.git?.getBranchState) {
      setBranchState(null);
      return;
    }

    let cancelled = false;
    void (async () => {
      try {
        const next = await window.electronAPI!.git.getBranchState(reviewPath);
        if (!cancelled) setBranchState(next);
      } catch {
        if (!cancelled) setBranchState(null);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [reviewPath, mission?.updatedAt]);

  useEffect(() => {
    if (
      activeTab !== "diff" ||
      !reviewPath ||
      !branchState ||
      !window.electronAPI?.git?.getFileDiffHead
    ) {
      return;
    }

    let cancelled = false;
    setDiffState((prev) => ({ ...prev, loading: true, error: undefined }));

    void (async () => {
      try {
        const changedFiles = branchState.changedFiles.slice(0, 20);
        const localChangedFiles = new Set([
          ...branchState.untracked,
          ...branchState.staged,
          ...branchState.unstaged,
        ]);
        const canCompareAgainstBase = Boolean(
          branchState.defaultBranch &&
            window.electronAPI?.git?.getFileDiffAgainstBase,
        );
        const files = await Promise.all(
          changedFiles.map(async (filePath) => {
            let diff = "";
            if (canCompareAgainstBase && branchState.defaultBranch) {
              diff = await window.electronAPI!.git.getFileDiffAgainstBase(
                reviewPath,
                filePath,
                branchState.defaultBranch,
              );
            }

            if (!diff.trim() && (localChangedFiles.has(filePath) || !canCompareAgainstBase)) {
              diff = await window.electronAPI!.git.getFileDiffHead(
                reviewPath,
                filePath,
              );
            }
            return {
              path: filePath,
              diff: diff || "Sem diff textual disponível para este arquivo.",
            };
          }),
        );
        if (!cancelled) {
          setDiffState({
            loading: false,
            files,
          });
        }
      } catch (error) {
        if (!cancelled) {
          setDiffState({
            loading: false,
            files: [],
            error: error instanceof Error ? error.message : "Falha ao carregar diff",
          });
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [activeTab, reviewPath, branchState, mission?.updatedAt]);

  useEffect(() => {
    if (activeTab === "logs" && shouldHideLogsTab) {
      setActiveTab("terminal");
    }
  }, [activeTab, shouldHideLogsTab]);

  useEffect(() => {
    if (!commitDialogOpen || !reviewPath || !window.electronAPI?.git) {
      if (!commitDialogOpen) setCommitDialogStatus(null);
      return;
    }

    let cancelled = false;
    setCommitDialogStatus(null);
    window.electronAPI.git
      .getStatus(reviewPath)
      .then((status) => {
        if (!cancelled) setCommitDialogStatus(status);
      })
      .catch(() => {
        if (!cancelled) setCommitDialogStatus(null);
      });

    return () => {
      cancelled = true;
    };
  }, [commitDialogOpen, reviewPath]);

  const handleCommit = async (message: string) => {
    if (!mission || !reviewPath || !window.electronAPI?.git) {
      toast.error("Commit indisponível");
      throw new Error("Commit indisponível");
    }

    try {
      const ok = await window.electronAPI.git.commit(reviewPath, message);
      if (!ok) {
        toast.error("Falha ao commitar. Verifique o status do repositório.");
        throw new Error("Falha ao commitar");
      }
      await update(mission.id, { isCommitted: true });
      await refreshMissions();
      toast.success("Commit realizado");
    } catch (error) {
      const msg = error instanceof Error ? error.message : "Erro desconhecido";
      toast.error(`Falha ao commitar: ${msg}`);
      throw error;
    }
  };

  const handlePush = async () => {
    if (!mission || !reviewPath || !window.electronAPI?.git?.push) {
      toast.error("Push indisponível");
      return;
    }
    setIsPushing(true);
    try {
      const result = await window.electronAPI.git.push(reviewPath);
      if (result.success) {
        await update(mission.id, { isPushed: true });
        await refreshMissions();
        toast.success("Push realizado");
      } else {
        toast.error(result.error ?? "Falha ao fazer push.");
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : "Erro desconhecido";
      toast.error(`Falha ao fazer push: ${msg}`);
    } finally {
      setIsPushing(false);
    }
  };

  const handleMerge = async () => {
    if (!mission?.worktreePath || !window.electronAPI?.worktree?.mergeIntoMain) return;
    const confirmed = await confirmDialog({
      title: "Incorporar alterações no branch principal?",
      description:
        "O branch da missão será feito merge no branch principal (main/master) e o worktree será removido.",
      confirmLabel: "Incorporar",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    setIsWorktreeAction(true);
    try {
      const result = await window.electronAPI.worktree.mergeIntoMain(mission.id);
      if (result?.success) {
        toast.success("Alterações incorporadas ao branch principal");
        await refreshMissions();
      } else {
        toast.error(result?.error ?? "Erro ao incorporar");
      }
    } catch (error) {
      toast.error(`Erro: ${error instanceof Error ? error.message : "desconhecido"}`);
    } finally {
      setIsWorktreeAction(false);
    }
  };

  const handleDiscard = async () => {
    if (!mission?.worktreePath || !window.electronAPI?.worktree?.discard) return;
    const confirmed = await confirmDialog({
      title: "Descartar worktree?",
      description:
        "O worktree e o branch da missão serão removidos. As alterações não commitadas serão perdidas.",
      confirmLabel: "Descartar",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    setIsWorktreeAction(true);
    try {
      const result = await window.electronAPI.worktree.discard(mission.id);
      if (result?.success) {
        toast.success("Worktree descartado");
        await refreshMissions();
      } else {
        toast.error(result?.error ?? "Erro ao descartar");
      }
    } catch (error) {
      toast.error(`Erro: ${error instanceof Error ? error.message : "desconhecido"}`);
    } finally {
      setIsWorktreeAction(false);
    }
  };

  const isLoading = (projectId && projectsLoading) || (missionId && missionsLoading);

  if (isLoading) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <Loader2 className="h-12 w-12 animate-spin text-muted-foreground" />
        <p className="text-muted-foreground">Carregando tarefa...</p>
      </div>
    );
  }

  if (!mission || !project) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <p className="text-muted-foreground">Tarefa não encontrada</p>
        <Button variant="outline" onClick={() => navigate(`/project/${projectId}/agents`)}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          Voltar ao projeto
        </Button>
      </div>
    );
  }

  if (mission.missionType !== "agents_cli") {
    navigate(`/project/${projectId}/mission/${missionId}`, { replace: true });
    return null;
  }

  return (
    <div className="flex h-full flex-col">
      <header className="shrink-0 border-b border-border px-6 py-4">
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="icon" asChild>
            <Link to={`/project/${projectId}/agents`}>
              <ArrowLeft className="h-4 w-4" />
            </Link>
          </Button>
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="truncate text-lg font-semibold">{mission.title}</h1>
              <Badge variant={getTaskStatusVariant(taskStatusLabel)}>{taskStatusLabel}</Badge>
            </div>
            {mission.description && (
              <p className="mt-0.5 line-clamp-2 text-sm text-muted-foreground">
                {mission.description}
              </p>
            )}
            <div className="mt-3 rounded-lg border bg-muted/10 p-3">
              <div className="flex flex-wrap items-start gap-3">
                <div className="min-w-0 flex-1">
                  <p className="text-[11px] uppercase tracking-wide text-muted-foreground">Branch real</p>
                  <p className="mt-1 break-all font-mono text-xs">
                    {currentBranchLabel}
                  </p>
                  {friendlyBranchLabel && (
                    <p className="mt-1 text-[11px] text-muted-foreground">
                      Nome amigável: {friendlyBranchLabel}
                    </p>
                  )}
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-7 px-2"
                    disabled={!branchState?.branch && !mission.worktreeBranch}
                    onClick={async () => {
                      const branchToCopy = branchState?.branch ?? mission.worktreeBranch;
                      if (!branchToCopy) return;
                      try {
                        await navigator.clipboard.writeText(branchToCopy);
                        toast.success("Nome da branch copiado");
                      } catch {
                        toast.error("Não foi possível copiar a branch");
                      }
                    }}
                  >
                    <Copy className="mr-1 h-3.5 w-3.5" />
                    Copiar branch
                  </Button>
                </div>
              </div>
              {reviewPath && (
                <p className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
                  {reviewPath}
                </p>
              )}
              {mission.context?.agentSession?.lastActivityAt && (
                <p className="mt-2 text-xs text-muted-foreground">
                  Última atividade{" "}
                  {formatDistanceToNow(
                    new Date(mission.context.agentSession.lastActivityAt),
                    { addSuffix: true, locale: ptBR },
                  )}
                </p>
              )}
            </div>
          </div>
        </div>
      </header>

      <div className="flex-1 overflow-hidden">
        <Tabs value={activeTab} onValueChange={setActiveTab} className="h-full flex flex-col">
          <div className="border-b border-border px-6">
            <TabsList className="h-12">
              <TabsTrigger value="terminal" className="gap-2">
                <Terminal className="h-4 w-4" />
                Terminal
              </TabsTrigger>
              {!shouldHideLogsTab && (
                <TabsTrigger value="logs" className="gap-2">
                  <MessageSquare className="h-4 w-4" />
                  Logs
                  {logs.length > 0 && (
                    <span className="ml-1 rounded bg-muted px-1.5 py-0.5 text-xs">
                      {logs.length}
                    </span>
                  )}
                </TabsTrigger>
              )}
              <TabsTrigger value="diff" className="gap-2">
                <FileText className="h-4 w-4" />
                Diff
                {branchState && branchState.changedFiles.length > 0 && (
                  <span className="ml-1 rounded bg-muted px-1.5 py-0.5 text-xs">
                    {branchState.changedFiles.length}
                  </span>
                )}
              </TabsTrigger>
            </TabsList>
          </div>

          <TabsContent
            value="terminal"
            className="flex-1 flex flex-col overflow-hidden p-0 mt-0 data-[state=active]:flex data-[state=inactive]:hidden"
          >
            {embeddedTerminalCwd ? (
              <div className="flex-1 min-h-0 p-4">
                <EmbeddedTerminal
                  cwd={embeddedTerminalCwd}
                  command={embeddedTerminalCommand}
                  args={embeddedTerminalArgs}
                  onClose={() => setEmbeddedTerminalCwd(null)}
                  title={mission.title}
                  missionId={missionId}
                />
              </div>
            ) : (
              <div className="flex flex-1 flex-col items-center justify-center gap-4 p-6 text-center">
                <Terminal className="h-12 w-12 text-muted-foreground" />
                <p className="text-sm text-muted-foreground">
                  Terminal da tarefa no diretório da worktree. Clique para iniciar.
                </p>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={openTerminal}
                  disabled={isOpeningTerminal}
                >
                  {isOpeningTerminal ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Terminal className="mr-2 h-4 w-4" />
                  )}
                  Abrir no terminal
                </Button>
              </div>
            )}
          </TabsContent>

          {!shouldHideLogsTab && (
            <TabsContent
              value="logs"
              className="flex-1 overflow-auto p-6 mt-0 data-[state=active]:block data-[state=inactive]:hidden"
            >
              <div className="space-y-2">
                {logs.length === 0 ? (
                  <p className="text-sm text-muted-foreground">Nenhum log ainda.</p>
                ) : (
                  [...logs]
                    .sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime())
                    .map((log) => {
                      const config = logTypeConfig[log.type] ?? logTypeConfig.info;
                      const Icon = config.icon;
                      return (
                        <div
                          key={log.id}
                          className="flex gap-3 rounded-lg border border-border bg-card p-3"
                        >
                          <Icon className={`h-5 w-5 shrink-0 ${config.color}`} />
                          <div className="min-w-0 flex-1">
                            <p className="text-sm">{log.content}</p>
                            <div className="mt-1 flex items-center gap-3 text-xs text-muted-foreground">
                              <span className="flex items-center gap-1">
                                <Clock className="h-3 w-3" />
                                {format(new Date(log.createdAt), "HH:mm:ss")}
                              </span>
                            </div>
                          </div>
                        </div>
                      );
                    })
                )}
              </div>
            </TabsContent>
          )}

          <TabsContent
            value="diff"
            className="flex-1 overflow-auto p-6 mt-0 data-[state=active]:block data-[state=inactive]:hidden"
          >
            {!reviewPath ? (
              <p className="text-sm text-muted-foreground">
                Esta tarefa ainda não possui worktree pronta para revisão.
              </p>
            ) : (
              <div className="space-y-4">
                <div className="rounded-lg border bg-muted/10 p-3">
                  <div className="flex flex-wrap items-center gap-3 text-sm">
                    <span className="inline-flex items-center gap-1">
                      <GitBranch className="h-4 w-4" />
                      {branchState?.branch ?? mission.worktreeBranch ?? "branch não identificada"}
                    </span>
                    {branchState?.defaultBranch && (
                      <span className="text-muted-foreground">
                        comparando com `{branchState.defaultBranch}`
                      </span>
                    )}
                  </div>
                  <p className="mt-2 text-sm text-muted-foreground">
                    {taskStatusLabel === "Em execução"
                      ? "As alterações abaixo refletem o estado atual da branch enquanto o agente ainda está trabalhando."
                      : "As alterações abaixo mostram o que esta branch mudou em relação à base do projeto."}
                  </p>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!reviewPath}
                      onClick={() => setCommitDialogOpen(true)}
                    >
                      <GitCommit className="mr-2 h-4 w-4" />
                      Commit
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!reviewPath || isPushing}
                      onClick={() => void handlePush()}
                    >
                      {isPushing ? (
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      ) : (
                        <Upload className="mr-2 h-4 w-4" />
                      )}
                      Push
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!mission.worktreePath || isWorktreeAction}
                      onClick={() => void handleMerge()}
                    >
                      <GitMerge className="mr-2 h-4 w-4" />
                      Incorporar
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={!mission.worktreePath || isWorktreeAction}
                      onClick={() => void handleDiscard()}
                    >
                      <Trash2 className="mr-2 h-4 w-4" />
                      Descartar
                    </Button>
                  </div>
                </div>

                {diffState.loading ? (
                  <div className="flex items-center gap-2 rounded-lg border p-4 text-sm text-muted-foreground">
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Carregando alterações da branch...
                  </div>
                ) : diffState.error ? (
                  <div className="rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-sm text-destructive">
                    {diffState.error}
                  </div>
                ) : diffState.files.length === 0 ? (
                  <div className="rounded-lg border p-4 text-sm text-muted-foreground">
                    Nenhuma alteração detectada para exibir no diff.
                  </div>
                ) : (
                  diffState.files.map((file) => (
                    <div key={file.path} className="rounded-lg border bg-card">
                      <div className="border-b px-4 py-2 font-mono text-xs">{file.path}</div>
                      <DiffCodeBlock content={file.diff} maxHeightClassName="max-h-[420px]" />
                    </div>
                  ))
                )}
              </div>
            )}
          </TabsContent>
        </Tabs>
      </div>
      <CommitDialog
        open={commitDialogOpen}
        onOpenChange={setCommitDialogOpen}
        defaultMessage={`feat: ${mission.title}`}
        onCommit={handleCommit}
        projectPath={reviewPath ?? ""}
        status={commitDialogStatus}
        onPushComplete={() => void refreshMissions()}
      />
    </div>
  );
}
